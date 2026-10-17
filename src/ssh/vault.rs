//! Passphrase wrapped SQLCipher archive keys for an SSH user's private profile.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

use anyhow::{Context, Result, ensure};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"ZAPSSH01";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;
const TAG_LEN: usize = 16;
const HEADER_LEN: usize = 8 + 12 + SALT_LEN + NONCE_LEN;
const FILE_LEN: usize = HEADER_LEN + KEY_LEN + TAG_LEN;
const MEMORY_KIB: u32 = 64 * 1024;
const ITERATIONS: u32 = 3;

/// Opens a passphrase vault or creates a random archive key and stores it
/// atomically. It never creates or modifies the archive itself.
pub struct Vault;

impl Vault {
    pub fn open_or_create(
        path: &Path,
        archive_path: &Path,
        passphrase: &str,
    ) -> Result<Zeroizing<[u8; KEY_LEN]>> {
        ensure!(!passphrase.is_empty(), "Passphrase must not be empty");
        ensure!(passphrase.len() <= 4096, "Passphrase is too long");
        match fs::symlink_metadata(path) {
            Ok(_) => Self::open_existing(path, passphrase),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                ensure!(
                    !archive_path.exists(),
                    "Archive exists but its passphrase key is missing; refusing to replace it"
                );
                Self::create(path, passphrase)
            }
            Err(error) => Err(error).context("Could not inspect archive key file"),
        }
    }

    fn open_existing(path: &Path, passphrase: &str) -> Result<Zeroizing<[u8; KEY_LEN]>> {
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
            .context("Could not open archive key file")?;
        let metadata = file.metadata()?;
        ensure!(metadata.is_file(), "Archive key is not a regular file");
        // Keep malformed files bounded before allocating or running Argon2.
        ensure!(
            metadata.len() == FILE_LEN as u64,
            "Invalid archive key file size"
        );
        ensure!(
            metadata.uid() == current_uid(),
            "Archive key belongs to another Unix user"
        );
        ensure!(
            metadata.mode() & 0o777 == 0o600,
            "Archive key permissions must be 0600"
        );
        let mut bytes = [0u8; FILE_LEN];
        file.read_exact(&mut bytes)
            .context("Could not read archive key file")?;
        Self::decode(&bytes, passphrase)
    }

    fn create(path: &Path, passphrase: &str) -> Result<Zeroizing<[u8; KEY_LEN]>> {
        let parent = path.parent().context("Archive key path has no parent")?;
        let parent_meta =
            fs::symlink_metadata(parent).context("Could not inspect key directory")?;
        ensure!(
            parent_meta.is_dir() && !parent_meta.file_type().is_symlink(),
            "Key directory must be a real directory"
        );
        ensure!(
            parent_meta.uid() == current_uid(),
            "Key directory belongs to another Unix user"
        );
        ensure!(
            parent_meta.mode() & 0o022 == 0,
            "Key directory is writable by other users"
        );

        let mut key = Zeroizing::new([0u8; KEY_LEN]);
        let mut salt = [0u8; SALT_LEN];
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::fill(&mut *key).context("Could not generate archive key")?;
        getrandom::fill(&mut salt).context("Could not generate archive key salt")?;
        getrandom::fill(&mut nonce).context("Could not generate archive key nonce")?;
        let bytes = Self::encode(&key, &salt, &nonce, passphrase)?;

        let mut suffix = [0u8; 8];
        getrandom::fill(&mut suffix).context("Could not generate temporary key name")?;
        let temp = parent.join(format!(".zapfast-key-{}", u64::from_ne_bytes(suffix)));
        let result = Self::write_new(path, &temp, &bytes);
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result?;
        let verified = Self::open_existing(path, passphrase)
            .context("Could not verify newly written archive key")?;
        ensure!(
            *key == *verified,
            "New archive key did not read back correctly"
        );
        Ok(verified)
    }

    fn write_new(path: &Path, temp: &Path, bytes: &[u8; FILE_LEN]) -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(temp)
            .context("Could not create temporary archive key")?;
        file.write_all(bytes)
            .context("Could not write archive key")?;
        file.sync_all().context("Could not sync archive key")?;
        drop(file);
        // A hard link makes publication atomic and fails rather than replacing
        // a key created by another process.
        fs::hard_link(temp, path).context("Could not publish archive key")?;
        fs::remove_file(temp).context("Could not remove temporary archive key")?;
        let parent = path.parent().context("Archive key path has no parent")?;
        File::open(parent)?
            .sync_all()
            .context("Could not sync key directory")?;
        Ok(())
    }

    fn encode(
        key: &[u8; KEY_LEN],
        salt: &[u8; SALT_LEN],
        nonce: &[u8; NONCE_LEN],
        passphrase: &str,
    ) -> Result<[u8; FILE_LEN]> {
        let mut derived = Zeroizing::new([0u8; KEY_LEN]);
        derive(passphrase, salt, MEMORY_KIB, ITERATIONS, &mut derived)?;
        let cipher = XChaCha20Poly1305::new((&*derived).into());
        let encrypted = cipher
            .encrypt(XNonce::from_slice(nonce), key.as_slice())
            .map_err(|_| anyhow::anyhow!("Could not encrypt archive key"))?;
        let mut out = [0u8; FILE_LEN];
        out[..8].copy_from_slice(MAGIC);
        out[8..12].copy_from_slice(&MEMORY_KIB.to_be_bytes());
        out[12..16].copy_from_slice(&ITERATIONS.to_be_bytes());
        out[16..20].copy_from_slice(&1u32.to_be_bytes());
        out[20..20 + SALT_LEN].copy_from_slice(salt);
        out[20 + SALT_LEN..HEADER_LEN].copy_from_slice(nonce);
        out[HEADER_LEN..].copy_from_slice(&encrypted);
        Ok(out)
    }

    fn decode(bytes: &[u8; FILE_LEN], passphrase: &str) -> Result<Zeroizing<[u8; KEY_LEN]>> {
        ensure!(&bytes[..8] == MAGIC, "Unsupported archive key version");
        let memory = u32::from_be_bytes(bytes[8..12].try_into()?);
        let iterations = u32::from_be_bytes(bytes[12..16].try_into()?);
        let lanes = u32::from_be_bytes(bytes[16..20].try_into()?);
        ensure!(
            (64 * 1024..=128 * 1024).contains(&memory),
            "Archive key memory cost is out of bounds"
        );
        ensure!(
            (1..=6).contains(&iterations),
            "Archive key iteration cost is out of bounds"
        );
        ensure!(lanes == 1, "Archive key parallelism is unsupported");
        let salt: &[u8; SALT_LEN] = bytes[20..20 + SALT_LEN].try_into()?;
        let nonce = XNonce::from_slice(&bytes[20 + SALT_LEN..HEADER_LEN]);
        let mut derived = Zeroizing::new([0u8; KEY_LEN]);
        derive(passphrase, salt, memory, iterations, &mut derived)?;
        let cipher = XChaCha20Poly1305::new((&*derived).into());
        let plaintext = cipher
            .decrypt(nonce, &bytes[HEADER_LEN..])
            .map_err(|_| anyhow::anyhow!("Passphrase did not unlock the archive key"))?;
        ensure!(
            plaintext.len() == KEY_LEN,
            "Invalid decrypted archive key length"
        );
        let mut key = Zeroizing::new([0u8; KEY_LEN]);
        key.copy_from_slice(&plaintext);
        Ok(key)
    }
}

fn derive(
    passphrase: &str,
    salt: &[u8; SALT_LEN],
    memory: u32,
    iterations: u32,
    output: &mut [u8; KEY_LEN],
) -> Result<()> {
    let params = Params::new(memory, iterations, 1, Some(KEY_LEN))
        .map_err(|error| anyhow::anyhow!("Invalid archive key derivation parameters: {error}"))?;
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(passphrase.as_bytes(), salt, output)
        .map_err(|error| anyhow::anyhow!("Could not derive archive key: {error}"))
}

fn current_uid() -> u32 {
    // SAFETY: geteuid has no preconditions and returns this process's UID.
    unsafe { libc::geteuid() }
}

#[cfg(test)]
mod tests {
    use super::{FILE_LEN, MAGIC, Vault};

    #[test]
    fn key_reopens() {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("archive.key");
        let archive = temp.path().join("archive.db");
        let first = Vault::open_or_create(&vault, &archive, "correct horse battery").unwrap();
        std::fs::write(&archive, b"encrypted archive fixture").unwrap();
        let second = Vault::open_or_create(&vault, &archive, "correct horse battery").unwrap();
        assert_eq!(&*first, &*second);
    }

    #[test]
    fn wrong_passphrase_preserves_files() {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("archive.key");
        let archive = temp.path().join("archive.db");
        Vault::open_or_create(&vault, &archive, "correct horse battery").unwrap();
        std::fs::write(&archive, b"encrypted archive fixture").unwrap();
        let before_vault = std::fs::read(&vault).unwrap();
        let before_archive = std::fs::read(&archive).unwrap();
        assert!(Vault::open_or_create(&vault, &archive, "wrong").is_err());
        assert_eq!(std::fs::read(vault).unwrap(), before_vault);
        assert_eq!(std::fs::read(archive).unwrap(), before_archive);
    }

    #[test]
    fn missing_vault_preserves_archive() {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("archive.key");
        let archive = temp.path().join("archive.db");
        std::fs::write(&archive, b"existing encrypted archive").unwrap();
        let before = std::fs::read(&archive).unwrap();
        assert!(Vault::open_or_create(&vault, &archive, "passphrase").is_err());
        assert_eq!(std::fs::read(archive).unwrap(), before);
        assert!(!vault.exists());
    }

    #[test]
    fn malformed_vault_is_bounded() {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("archive.key");
        let archive = temp.path().join("archive.db");
        std::fs::write(&vault, b"ZAPSSH01").unwrap();
        assert!(Vault::open_or_create(&vault, &archive, "passphrase").is_err());
        assert!(!archive.exists());
        std::fs::write(&vault, vec![0; 1024 * 1024]).unwrap();
        assert!(Vault::open_or_create(&vault, &archive, "passphrase").is_err());
        assert!(!archive.exists());
        let mut excessive_cost = vec![0; FILE_LEN];
        excessive_cost[..8].copy_from_slice(MAGIC);
        excessive_cost[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
        excessive_cost[12..16].copy_from_slice(&u32::MAX.to_be_bytes());
        excessive_cost[16..20].copy_from_slice(&1u32.to_be_bytes());
        std::fs::write(&vault, excessive_cost).unwrap();
        assert!(Vault::open_or_create(&vault, &archive, "passphrase").is_err());
        assert!(!archive.exists());
    }

    #[test]
    fn failed_create_leaves_no_archive() {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("missing-parent/archive.key");
        let archive = temp.path().join("archive.db");
        assert!(Vault::open_or_create(&vault, &archive, "passphrase").is_err());
        assert!(!archive.exists());
        assert!(!vault.exists());
    }
}
