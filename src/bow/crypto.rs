//! BOW Crypto — AES-256-GCM encrypt/decrypt + argon2id KDF

use aes_gcm::aead::Aead;
use aes_gcm::aead::rand_core::OsRng;
use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use argon2::Argon2;
use zeroize::Zeroizing;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

/// Derive a 32-byte key from a passphrase using argon2id.
pub fn derive_key(passphrase: &[u8], salt: &[u8; SALT_LEN]) -> Zeroizing<[u8; KEY_LEN]> {
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(19456, 2, 1, Some(KEY_LEN)).unwrap(),
    )
    .hash_password_into(passphrase, salt, &mut *key)
    .expect("argon2id key derivation failed");
    key
}

/// Generate a random 16-byte salt.
pub fn generate_salt() -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    salt
}

/// Encrypt plaintext with AES-256-GCM.
/// Returns: nonce (12 bytes) || ciphertext || tag (16 bytes)
pub fn encrypt(
    key: &[u8; KEY_LEN],
    plaintext: &[u8],
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let cipher = Aes256Gcm::new_from_slice(key)?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| format!("encrypt error: {e}"))?;
    let mut result = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);
    Ok(result)
}

/// Decrypt ciphertext with AES-256-GCM.
/// Input: nonce (12 bytes) || ciphertext || tag (16 bytes)
pub fn decrypt(key: &[u8; KEY_LEN], data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if data.len() < NONCE_LEN + 16 {
        return Err("ciphertext too short".into());
    }
    let cipher = Aes256Gcm::new_from_slice(key)?;
    let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);
    Ok(cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("decrypt error: {e}"))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encrypt_decrypt() {
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(b"0123456789abcdef0123456789abcdef");
        let plaintext = b"hello world";
        let encrypted = encrypt(&key, plaintext).unwrap();
        let decrypted = decrypt(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn wrong_key_fails() {
        let mut key1 = [0u8; KEY_LEN];
        key1.copy_from_slice(b"0123456789abcdef0123456789abcdef");
        let mut key2 = [0u8; KEY_LEN];
        key2.copy_from_slice(b"fedcba9876543210fedcba9876543210");
        let plaintext = b"secret data";
        let encrypted = encrypt(&key1, plaintext).unwrap();
        let result = decrypt(&key2, &encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn derive_key_deterministic() {
        let salt = [42u8; SALT_LEN];
        let k1 = derive_key(b"passphrase", &salt);
        let k2 = derive_key(b"passphrase", &salt);
        assert_eq!(*k1, *k2);
    }

    #[test]
    fn different_salts_different_keys() {
        let s1 = [1u8; SALT_LEN];
        let s2 = [2u8; SALT_LEN];
        let k1 = derive_key(b"pass", &s1);
        let k2 = derive_key(b"pass", &s2);
        assert_ne!(*k1, *k2);
    }
}
