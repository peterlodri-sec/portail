//! BOW Key — master key resolution (env, file, prompt)

use std::path::PathBuf;
use zeroize::Zeroizing;

const KEY_LEN: usize = 32;

/// Resolve master key from environment, file, or interactive prompt.
/// Priority: env > file > prompt.
pub fn resolve_master_key() -> Result<Zeroizing<[u8; KEY_LEN]>, super::BowError> {
    // 1. Environment variable
    if let Ok(hex) = std::env::var("PORTAIL_MASTER_KEY") {
        let key = parse_hex_key(&hex)?;
        return Ok(key);
    }

    // 2. File
    let key_path = key_file_path();
    if key_path.exists() {
        let hex = std::fs::read_to_string(&key_path).map_err(super::BowError::Io)?;
        let key = parse_hex_key(hex.trim())?;
        return Ok(key);
    }

    // 3. Interactive prompt
    eprint!("BOW master key: ");
    let passphrase = rpassword::read_password()
        .map_err(|e| super::BowError::NoMasterKey(format!("failed to read password: {e}")))?;
    if passphrase.is_empty() {
        return Err(super::BowError::NoMasterKey("empty passphrase".into()));
    }
    // When using passphrase, derive key with a fixed salt from meta table
    // For init, caller generates salt. For open, caller reads salt from DB.
    // Here we return a special marker that the caller handles.
    Ok(Zeroizing::new([0u8; KEY_LEN])) // placeholder — caller detects and re-derives
}

/// Parse a 64-character hex string into a 32-byte key.
fn parse_hex_key(hex: &str) -> Result<Zeroizing<[u8; KEY_LEN]>, super::BowError> {
    if hex.len() != KEY_LEN * 2 {
        return Err(super::BowError::NoMasterKey(format!(
            "key must be {} hex chars, got {}",
            KEY_LEN * 2,
            hex.len()
        )));
    }
    let bytes =
        hex::decode(hex).map_err(|e| super::BowError::NoMasterKey(format!("invalid hex: {e}")))?;
    let mut key = [0u8; KEY_LEN];
    key.copy_from_slice(&bytes);
    Ok(Zeroizing::new(key))
}

/// Default path for master key file.
pub fn key_file_path() -> PathBuf {
    dirs_or_home()
        .join(".config")
        .join("portail")
        .join("master.key")
}

/// Get user's home directory.
fn dirs_or_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_key_valid() {
        let hex = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        let key = parse_hex_key(hex).unwrap();
        assert_eq!(key[0], 0x00);
        assert_eq!(key[31], 0xff);
    }

    #[test]
    fn parse_hex_key_wrong_length() {
        let hex = "001122";
        assert!(parse_hex_key(hex).is_err());
    }

    #[test]
    fn parse_hex_key_invalid_hex() {
        let hex = "zz112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        assert!(parse_hex_key(hex).is_err());
    }
}
