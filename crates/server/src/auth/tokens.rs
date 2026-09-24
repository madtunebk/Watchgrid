//! Session tokens: 256 random bits in the cookie; only a BLAKE2b hash is stored.

use aes_gcm::aead::OsRng;
use aes_gcm::aead::rand_core::RngCore;
use blake2::{Blake2b, Digest, digest::consts::U32};

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    OsRng.fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// A new cookie token.
pub fn new_token() -> String {
    random_hex(32)
}

/// Public session handle (safe to show and use for revoking).
pub fn new_session_id() -> String {
    format!("ses-{}", random_hex(8))
}

pub fn hash(token: &str) -> Vec<u8> {
    Blake2b::<U32>::digest(token.as_bytes()).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random_and_hashed() {
        let (a, b) = (new_token(), new_token());
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
        assert_eq!(hash(&a), hash(&a));
        assert_ne!(hash(&a), hash(&b));
        assert_eq!(hash(&a).len(), 32);
    }
}
