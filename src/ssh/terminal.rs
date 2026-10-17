use std::io::{self, Stdout, Write};

use anyhow::Context;
use crossterm::{
    event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use super::state::{InputAction, SshState};

pub struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    raw: bool,
    alternate: bool,
}

impl TerminalGuard {
    pub fn enter() -> anyhow::Result<Self> {
        enable_raw_mode().context("enable terminal raw mode")?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error).context("enter alternate screen");
        }
        let terminal = match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => terminal,
            Err(error) => {
                let mut stdout = io::stdout();
                let _ = execute!(stdout, LeaveAlternateScreen);
                let _ = disable_raw_mode();
                return Err(error).context("initialize terminal UI");
            }
        };
        Ok(Self {
            terminal,
            raw: true,
            alternate: true,
        })
    }

    pub fn draw(&mut self, state: &SshState) -> anyhow::Result<()> {
        self.terminal
            .draw(|frame| super::render::draw(frame, state))?;
        Ok(())
    }

    pub fn read_key(&mut self) -> anyhow::Result<Option<KeyEvent>> {
        if let CrosstermEvent::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                return Ok(Some(key));
            }
        }
        Ok(None)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore(&mut self.terminal, &mut self.raw, &mut self.alternate);
    }
}

fn restore<W: Write>(
    terminal: &mut Terminal<CrosstermBackend<W>>,
    raw: &mut bool,
    alternate: &mut bool,
) {
    let _ = terminal.show_cursor();
    if *alternate {
        let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
        *alternate = false;
    }
    if *raw {
        let _ = disable_raw_mode();
        *raw = false;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputMode {
    Browse,
    Compose,
    Search,
}

#[derive(Debug, Default)]
pub struct KeyInput {
    mode: InputMode,
    buffer: String,
}

impl Default for InputMode {
    fn default() -> Self {
        Self::Browse
    }
}

impl KeyInput {
    pub fn handle(&mut self, key: KeyEvent, state: &SshState) -> Vec<InputAction> {
        if key.code == KeyCode::Esc {
            if self.mode != InputMode::Browse {
                self.mode = InputMode::Browse;
                self.buffer.clear();
                return vec![];
            }
            return vec![InputAction::Back];
        }
        if self.mode != InputMode::Browse {
            match key.code {
                KeyCode::Enter if self.mode == InputMode::Search => {
                    self.mode = InputMode::Browse;
                    return vec![InputAction::Search(self.buffer.clone())];
                }
                KeyCode::Enter => {
                    return vec![InputAction::Compose(self.buffer.clone()), InputAction::Send];
                }
                KeyCode::Backspace => {
                    self.buffer.pop();
                    return vec![InputAction::Compose(self.buffer.clone())];
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.buffer.push(c);
                    return if self.mode == InputMode::Compose {
                        vec![InputAction::Compose(self.buffer.clone())]
                    } else {
                        vec![]
                    };
                }
                _ => return vec![],
            }
        }
        match key.code {
            KeyCode::Char('q') => vec![InputAction::Quit],
            KeyCode::Char('/') => {
                self.mode = InputMode::Search;
                self.buffer.clear();
                vec![]
            }
            KeyCode::Char('i') => {
                self.mode = InputMode::Compose;
                self.buffer = state.draft.clone();
                vec![]
            }
            KeyCode::Char('r') => vec![InputAction::QuoteSelected],
            KeyCode::Up | KeyCode::Char('k') => vec![InputAction::SelectChat(
                state.selected_chat.saturating_sub(1),
            )],
            KeyCode::Down | KeyCode::Char('j') => vec![InputAction::SelectChat(
                state.selected_chat.saturating_add(1),
            )],
            KeyCode::Enter if state.open_chat.is_some() => vec![],
            KeyCode::Enter => vec![InputAction::OpenSelected],
            KeyCode::PageUp => vec![InputAction::OlderPage],
            KeyCode::Backspace => vec![InputAction::Back],
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventKind};

    #[test]
    fn input_error_restores_terminal_flags() {
        let mut raw = true;
        let mut alternate = true;
        let backend = CrosstermBackend::new(io::sink());
        let mut terminal = Terminal::new(backend).unwrap();
        restore(&mut terminal, &mut raw, &mut alternate);
        assert!(!raw && !alternate);
    }

    #[test]
    fn keys_enter_compose_and_search_modes() {
        let mut input = KeyInput::default();
        let state = SshState::new();
        let key = |code| KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        assert!(input.handle(key(KeyCode::Char('i')), &state).is_empty());
        assert_eq!(
            input.handle(key(KeyCode::Char('h')), &state),
            vec![InputAction::Compose("h".into())]
        );
        assert_eq!(
            input.handle(key(KeyCode::Enter), &state),
            vec![InputAction::Compose("h".into()), InputAction::Send]
        );
    }
}
