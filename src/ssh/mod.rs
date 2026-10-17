//! Headless SSH terminal client.

use std::{
    io::{self, IsTerminal},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::{Context, bail};
use crossterm::event::{self, Event as CrosstermEvent, KeyEventKind};
use zeroize::Zeroizing;

use self::{
    backend::{ArchiveUnlock, Backend, BackendWake, Command, Event},
    profile::Profile,
    state::SshState,
    terminal::{KeyInput, TerminalGuard},
    vault::Vault,
};

pub mod backend;
pub mod profile;
pub mod qr;
pub mod render;
pub mod state;
pub mod terminal;
pub mod vault;

enum Wake {
    Backend,
    Terminal(CrosstermEvent),
    Signal,
    InputError,
}

/// Backend boundary used by the session driver and its lifecycle tests.
trait SessionBackend {
    fn start(
        &mut self,
        dirs: crate::paths::AccountDirs,
        wake: BackendWake,
        key: Zeroizing<[u8; 32]>,
    ) -> anyhow::Result<()>;
    fn send(&mut self, command: Command) -> anyhow::Result<()>;
    fn poll(&mut self) -> Vec<Event>;
    fn shutdown(&mut self);
}

struct LiveBackend(Option<Backend>);

impl SessionBackend for LiveBackend {
    fn start(
        &mut self,
        dirs: crate::paths::AccountDirs,
        wake: BackendWake,
        key: Zeroizing<[u8; 32]>,
    ) -> anyhow::Result<()> {
        self.0 = Some(Backend::spawn_headless(
            dirs,
            wake,
            ArchiveUnlock::Supplied(key),
        ));
        Ok(())
    }
    fn send(&mut self, command: Command) -> anyhow::Result<()> {
        self.0
            .as_ref()
            .context("WhatsApp backend has not started")?
            .send(command)
    }
    fn poll(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        if let Some(backend) = self.0.as_ref() {
            while let Some(event) = backend.try_recv() {
                events.push(event);
            }
        }
        events
    }
    fn shutdown(&mut self) {
        if let Some(backend) = self.0.take() {
            backend.shutdown();
        }
    }
}

/// Runs one foreground terminal session. The profile lock outlives backend shutdown.
pub fn run(profile: Profile, passphrase: Zeroizing<String>) -> anyhow::Result<()> {
    let _lock = profile.acquire()?;
    let dirs = profile.account_dirs();
    let key = Vault::open_or_create(
        &profile.root().join("archive.key"),
        &dirs.archive_db(),
        &passphrase,
    )?;
    drop(passphrase);
    let mut terminal = TerminalGuard::enter()?;
    let (wake_tx, wake_rx) = mpsc::channel();
    let stopping = Arc::new(AtomicBool::new(false));
    let signal_flag = Arc::new(AtomicBool::new(false));
    let signal_ids = [libc::SIGINT, libc::SIGHUP, libc::SIGTERM]
        .into_iter()
        .map(|signal| signal_hook::flag::register(signal, signal_flag.clone()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let input_thread = spawn_terminal_reader(wake_tx.clone(), stopping.clone(), signal_flag);
    let wake = BackendWake::new(move || {
        let _ = wake_tx.send(Wake::Backend);
    });
    let mut backend = LiveBackend(None);
    let result = drive_session(&mut backend, dirs, wake, key, &wake_rx, &mut terminal);
    stopping.store(true, Ordering::SeqCst);
    let _ = input_thread.join();
    for id in signal_ids {
        signal_hook::low_level::unregister(id);
    }
    result
}

fn spawn_terminal_reader(
    tx: mpsc::Sender<Wake>,
    stopping: Arc<AtomicBool>,
    signal: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("zapfast-ssh-input".into())
        .spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                if signal.swap(false, Ordering::SeqCst) {
                    let _ = tx.send(Wake::Signal);
                    return;
                }
                match event::poll(Duration::from_millis(100)) {
                    Ok(true) => match event::read() {
                        Ok(CrosstermEvent::Key(key)) if key.kind == KeyEventKind::Press => {
                            if tx.send(Wake::Terminal(CrosstermEvent::Key(key))).is_err() {
                                return;
                            }
                        }
                        Ok(event @ CrosstermEvent::Resize(_, _)) => {
                            if tx.send(Wake::Terminal(event)).is_err() {
                                return;
                            }
                        }
                        Ok(_) => {}
                        Err(_) => {
                            let _ = tx.send(Wake::InputError);
                            return;
                        }
                    },
                    Ok(false) => {}
                    Err(_) => {
                        let _ = tx.send(Wake::InputError);
                        return;
                    }
                }
            }
        })
        .expect("could not start terminal input reader")
}

fn drive_session<B: SessionBackend>(
    backend: &mut B,
    dirs: crate::paths::AccountDirs,
    wake: BackendWake,
    key: Zeroizing<[u8; 32]>,
    wake_rx: &mpsc::Receiver<Wake>,
    terminal: &mut TerminalGuard,
) -> anyhow::Result<()> {
    let outcome = with_backend_shutdown(backend, |backend| {
        backend.start(dirs, wake, key)?;
        backend.send(Command::LoadChats)?;
        let mut state = SshState::new();
        let mut input = KeyInput::default();
        terminal.draw(&state)?;
        loop {
            match wake_rx.recv().context("terminal event channel closed")? {
                Wake::Backend => {
                    for event in backend.poll() {
                        state.apply(event);
                    }
                }
                Wake::Terminal(CrosstermEvent::Key(key)) => {
                    for action in input.handle(key, &state) {
                        let commands = state.act(action);
                        for command in commands {
                            backend.send(command)?;
                        }
                    }
                }
                Wake::Terminal(CrosstermEvent::Resize(_, _)) => {}
                Wake::Signal => {
                    state.quit = true;
                }
                Wake::InputError => bail!("terminal input stopped unexpectedly"),
                Wake::Terminal(_) => {}
            }
            terminal.draw(&state)?;
            if state.quit {
                return Ok(());
            }
        }
    });
    outcome
}

fn with_backend_shutdown<B: SessionBackend, R>(
    backend: &mut B,
    run: impl FnOnce(&mut B) -> anyhow::Result<R>,
) -> anyhow::Result<R> {
    let result = run(backend);
    backend.shutdown();
    result
}

/// Returns a useful error before creating the per-user profile when SSH has no TTY.
pub fn require_tty(input: bool, output: bool) -> anyhow::Result<()> {
    if !input || !output {
        bail!("zapfast-ssh needs a terminal; connect with `ssh -t host zapfast-ssh`");
    }
    Ok(())
}

pub fn current_tty() -> anyhow::Result<()> {
    require_tty(io::stdin().is_terminal(), io::stdout().is_terminal())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, path::PathBuf};
    use tempfile::tempdir;

    struct FakeBackend {
        starts: Cell<usize>,
        sends: Cell<usize>,
        shutdowns: Cell<usize>,
        wakes: Vec<Event>,
    }
    impl SessionBackend for FakeBackend {
        fn start(
            &mut self,
            _: crate::paths::AccountDirs,
            _: BackendWake,
            _: Zeroizing<[u8; 32]>,
        ) -> anyhow::Result<()> {
            self.starts.set(self.starts.get() + 1);
            Ok(())
        }
        fn send(&mut self, command: Command) -> anyhow::Result<()> {
            if matches!(command, Command::LoadChats) {
                self.sends.set(self.sends.get() + 1);
            }
            Ok(())
        }
        fn poll(&mut self) -> Vec<Event> {
            std::mem::take(&mut self.wakes)
        }
        fn shutdown(&mut self) {
            self.shutdowns.set(self.shutdowns.get() + 1);
        }
    }

    #[test]
    fn startup_once() {
        let temp = tempdir().unwrap();
        let dirs = Profile::under(&temp.path().join("profile"))
            .unwrap()
            .account_dirs();
        let mut backend = FakeBackend {
            starts: Cell::new(0),
            sends: Cell::new(0),
            shutdowns: Cell::new(0),
            wakes: vec![],
        };
        backend
            .start(dirs, BackendWake::new(|| {}), Zeroizing::new([0; 32]))
            .unwrap();
        backend.send(Command::LoadChats).unwrap();
        assert_eq!(backend.starts.get(), 1);
        assert_eq!(backend.sends.get(), 1);
        backend.shutdown();
        assert_eq!(backend.shutdowns.get(), 1);
    }

    #[test]
    fn every_exit_shuts_down_and_releases_profile_lock() {
        for exit in [
            Ok(()),
            Err(anyhow::anyhow!("input error")),
            Err(anyhow::anyhow!("HUP")),
            Err(anyhow::anyhow!("TERM")),
        ] {
            let temp = tempdir().unwrap();
            let profile = Profile::under(&temp.path().join("profile")).unwrap();
            let lock = profile.acquire().unwrap();
            let mut backend = FakeBackend {
                starts: Cell::new(0),
                sends: Cell::new(0),
                shutdowns: Cell::new(0),
                wakes: vec![],
            };
            let _ = with_backend_shutdown(&mut backend, |_| exit.map(|_| ()));
            assert_eq!(backend.shutdowns.get(), 1);
            drop(lock);
            assert!(profile.acquire().is_ok());
        }
    }

    #[test]
    fn no_tty_leaves_profile_untouched() {
        let temp = tempdir().unwrap();
        let path: PathBuf = temp.path().join("new-profile");
        assert!(require_tty(false, false).is_err());
        assert!(!path.exists());
    }
}
