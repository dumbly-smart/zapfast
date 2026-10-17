//! Private per-Unix-user storage and the single active SSH session lock.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};

use crate::model::AccountId;
use crate::paths::AccountDirs;

/// One Unix user's SSH client files. The caller must acquire its lock before
/// opening either database.
#[derive(Clone, Debug)]
pub struct Profile {
    root: PathBuf,
}

/// Holds the kernel lock until the handle is dropped or the process exits.
pub struct SessionLock {
    _file: File,
}

impl Profile {
    /// Uses this Unix user's XDG state directory, never the desktop profile.
    pub fn discover() -> Result<Self> {
        let home = std::env::var_os("HOME").context("HOME is not set")?;
        let home = PathBuf::from(home);
        ensure!(home.is_absolute(), "HOME must be an absolute path");
        let metadata = fs::symlink_metadata(&home).context("Could not inspect HOME")?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "HOME must be a real directory"
        );
        ensure!(
            metadata.uid() == current_uid(),
            "HOME belongs to a different Unix user"
        );
        let base = match std::env::var_os("XDG_STATE_HOME") {
            Some(base) => {
                let base = PathBuf::from(base);
                ensure!(
                    base.is_absolute(),
                    "XDG_STATE_HOME must be an absolute path"
                );
                base
            }
            None => home.join(".local/state"),
        };
        Self::under(&base.join("zapfast-ssh"))
    }

    /// Creates a private profile under an explicit path; useful for disposable tests.
    pub fn under(root: &Path) -> Result<Self> {
        ensure!(root.is_absolute(), "Profile path must be absolute");
        private_umask();
        let parent = root.parent().context("Profile path has no parent")?;
        fs::create_dir_all(parent).context("Could not create profile parent")?;
        let parent_meta =
            fs::symlink_metadata(parent).context("Could not inspect profile parent")?;
        ensure!(
            parent_meta.is_dir() && !parent_meta.file_type().is_symlink(),
            "Profile parent must be a real directory"
        );
        ensure!(
            parent_meta.uid() == current_uid(),
            "Profile parent belongs to a different Unix user"
        );
        ensure!(
            parent_meta.mode() & 0o022 == 0,
            "Profile parent is writable by other users"
        );
        match fs::symlink_metadata(root) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let mut builder = fs::DirBuilder::new();
                builder.mode(0o700);
                builder
                    .create(root)
                    .context("Could not create profile directory")?;
            }
            Err(error) => return Err(error).context("Could not inspect profile directory"),
        }
        check_private_directory(root)?;
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn lock_path(&self) -> PathBuf {
        self.root.join("session.lock")
    }

    /// Refuses a concurrent SSH login before either account database opens.
    pub fn acquire(&self) -> Result<SessionLock> {
        private_umask();
        check_private_directory(&self.root)?;
        let path = self.lock_path();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&path)
            .context("Could not open SSH session lock")?;
        let metadata = file.metadata()?;
        ensure!(metadata.is_file(), "SSH session lock is not a regular file");
        ensure!(
            metadata.uid() == current_uid(),
            "SSH session lock belongs to a different Unix user"
        );
        ensure!(
            metadata.mode() & 0o777 == 0o600,
            "SSH session lock permissions must be 0600"
        );
        // SAFETY: flock receives the live file descriptor held by SessionLock.
        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if result != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::EWOULDBLOCK) {
                bail!("ZapFast SSH is already running for this Unix user");
            }
            return Err(error).context("Could not acquire SSH session lock");
        }
        Ok(SessionLock { _file: file })
    }

    pub fn account_dirs(&self) -> AccountDirs {
        let id = AccountId::first();
        AccountDirs {
            state: self.root.join("state/accounts").join(id.as_str()),
            cache: self.root.join("cache/accounts").join(id.as_str()),
            id,
        }
    }
}

fn current_uid() -> u32 {
    // SAFETY: geteuid has no preconditions and returns the process identity.
    unsafe { libc::geteuid() }
}

fn private_umask() {
    // SAFETY: umask sets a process-wide creation policy before storage starts.
    unsafe { libc::umask(0o077) };
}

fn check_private_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path).context("Could not inspect profile directory")?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "Profile must be a real directory"
    );
    ensure!(
        metadata.uid() == current_uid(),
        "Profile belongs to a different Unix user"
    );
    ensure!(
        metadata.mode() & 0o777 == 0o700,
        "Profile permissions must be 0700"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Profile;
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn private_profile_modes() {
        let temp = tempfile::tempdir().unwrap();
        let profile = Profile::under(&temp.path().join("person")).unwrap();
        let lock = profile.acquire().unwrap();
        assert_eq!(
            std::fs::metadata(profile.root())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(profile.lock_path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        drop(lock);
    }

    #[test]
    fn second_lock_fails_until_first_drops() {
        let temp = tempfile::tempdir().unwrap();
        let profile = Profile::under(&temp.path().join("person")).unwrap();
        let first = profile.acquire().unwrap();
        assert!(profile.acquire().is_err());
        drop(first);
        assert!(profile.acquire().is_ok());
    }

    #[test]
    fn distinct_profiles_do_not_conflict() {
        let temp = tempfile::tempdir().unwrap();
        let a = Profile::under(&temp.path().join("a")).unwrap();
        let b = Profile::under(&temp.path().join("b")).unwrap();
        let _a = a.acquire().unwrap();
        let _b = b.acquire().unwrap();
        assert_ne!(a.account_dirs().state, b.account_dirs().state);
    }

    #[test]
    fn unsafe_root_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let open = temp.path().join("open");
        std::fs::create_dir(&open).unwrap();
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(Profile::under(&open).is_err());
        let link = temp.path().join("link");
        symlink(temp.path(), &link).unwrap();
        assert!(Profile::under(&link).is_err());
    }
}
