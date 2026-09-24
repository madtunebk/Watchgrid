//! Encryption of stored secrets (camera and ONVIF passwords now; provider
//! tokens later).
//!
//! AES-256-GCM with a random 96-bit nonce per value and associated data
//! naming what the secret belongs to (e.g. `camera:cam-front:password`), so a
//! ciphertext cannot be moved to another record. Stored layout:
//! `[version u8][nonce 12 bytes][ciphertext + tag]`.
//!
//! The 32-byte master key lives in `<data_dir>/master.key` (mode 0600),
//! created on first start. It is deliberately NOT derived from machine
//! identity, so backup/restore and hardware changes keep working.
//! Backups must include this file together with the database: without it
//! stored secrets cannot be decrypted and must be re-entered. The version
//! byte leaves room for key rotation.

use std::io::{self, Read, Write};
use std::path::Path;

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};

const KEY_FILE: &str = "master.key";
const VERSION: u8 = 1;
const NONCE_LEN: usize = 12;

#[derive(Debug, PartialEq, Eq)]
pub enum CredentialError {
    /// Wrong key, wrong associated data, or tampered ciphertext.
    Undecryptable,
    Malformed,
    /// Encryption refused the input (only possible for absurdly large values).
    Encrypt,
}

impl std::fmt::Display for CredentialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Undecryptable => "stored secret cannot be decrypted (wrong key or tampered data)",
            Self::Malformed => "stored secret is malformed",
            Self::Encrypt => "secret could not be encrypted",
        })
    }
}

pub struct CredentialStore {
    cipher: Aes256Gcm,
}

impl CredentialStore {
    pub fn from_key(key: &[u8; 32]) -> Self {
        Self { cipher: Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key)) }
    }

    /// Load the master key from `dir`, creating it (0600) on first run.
    pub fn load_or_create(dir: &Path) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(KEY_FILE);
        let mut key = [0u8; 32];
        match std::fs::File::open(&path) {
            Ok(mut f) => {
                let mut buf = Vec::new();
                f.read_to_end(&mut buf)?;
                if buf.len() != key.len() {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, format!("{} is not a 32-byte key", path.display())));
                }
                key.copy_from_slice(&buf);
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                key.copy_from_slice(&Aes256Gcm::generate_key(OsRng));
                let mut opts = std::fs::OpenOptions::new();
                opts.write(true).create_new(true);
                #[cfg(unix)]
                std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
                opts.open(&path)?.write_all(&key)?;
                tracing::info!("created credential master key at {}", path.display());
            }
            Err(e) => return Err(e),
        }
        Ok(Self::from_key(&key))
    }

    pub fn seal(&self, aad: &str, plaintext: &[u8]) -> Result<Vec<u8>, CredentialError> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = self
            .cipher
            .encrypt(&nonce, Payload { msg: plaintext, aad: aad.as_bytes() })
            .map_err(|_| CredentialError::Encrypt)?;
        let mut out = Vec::with_capacity(1 + NONCE_LEN + ciphertext.len());
        out.push(VERSION);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    pub fn open(&self, aad: &str, sealed: &[u8]) -> Result<Vec<u8>, CredentialError> {
        let (&version, rest) = sealed.split_first().ok_or(CredentialError::Malformed)?;
        if version != VERSION || rest.len() <= NONCE_LEN {
            return Err(CredentialError::Malformed);
        }
        let (nonce, ciphertext) = rest.split_at(NONCE_LEN);
        self.cipher
            .decrypt(Nonce::from_slice(nonce), Payload { msg: ciphertext, aad: aad.as_bytes() })
            .map_err(|_| CredentialError::Undecryptable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> CredentialStore {
        CredentialStore::from_key(&[7u8; 32])
    }

    #[test]
    fn round_trip() {
        let s = store();
        let sealed = s.seal("camera:a:password", b"hunter2").unwrap();
        assert_eq!(s.open("camera:a:password", &sealed).unwrap(), b"hunter2");
    }

    #[test]
    fn nonce_is_fresh_each_time() {
        let s = store();
        assert_ne!(s.seal("x", b"same").unwrap(), s.seal("x", b"same").unwrap());
    }

    #[test]
    fn ciphertext_is_bound_to_its_record() {
        let s = store();
        let sealed = s.seal("camera:a:password", b"hunter2").unwrap();
        assert_eq!(s.open("camera:b:password", &sealed), Err(CredentialError::Undecryptable));
    }

    #[test]
    fn tampering_is_detected() {
        let s = store();
        let mut sealed = s.seal("k", b"secret").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 1;
        assert_eq!(s.open("k", &sealed), Err(CredentialError::Undecryptable));
    }

    #[test]
    fn wrong_key_fails() {
        let sealed = store().seal("k", b"secret").unwrap();
        assert_eq!(CredentialStore::from_key(&[8u8; 32]).open("k", &sealed), Err(CredentialError::Undecryptable));
    }

    #[test]
    fn malformed_input_is_rejected() {
        assert_eq!(store().open("k", &[]), Err(CredentialError::Malformed));
        assert_eq!(store().open("k", &[9, 1, 2, 3]), Err(CredentialError::Malformed));
    }

    #[test]
    fn key_file_is_created_once_and_private() {
        let dir = std::env::temp_dir().join(format!("wg-cred-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = CredentialStore::load_or_create(&dir).unwrap();
        let sealed = a.seal("k", b"v").unwrap();
        let b = CredentialStore::load_or_create(&dir).unwrap();
        assert_eq!(b.open("k", &sealed).unwrap(), b"v");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join(KEY_FILE)).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
