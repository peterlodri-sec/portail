//! Bounded types for hot-path data structures.
//!
//! # v1.3 — Type Hardening
//!
//! Replaces unbounded `FxHashMap<String, String>` with bounded `BoundedMeta` type.
//! Enforces max key count, max key/value length, and limits memory exposure.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;

/// Maximum number of metadata entries allowed.
pub const MAX_METADATA_ENTRIES: usize = 16;

/// Maximum length of a metadata key in bytes.
pub const MAX_KEY_LEN: usize = 128;

/// Maximum length of a metadata value in bytes.
pub const MAX_VALUE_LEN: usize = 512;

/// Bounded metadata key-value store.
///
/// Replaces `FxHashMap<String, String>` on hot-path structures.
/// Enforces entry count, key length, and value length limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedMeta {
    inner: HashMap<String, String>,
}

impl BoundedMeta {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            inner: HashMap::with_capacity(cap.min(MAX_METADATA_ENTRIES)),
        }
    }

    pub fn insert(&mut self, key: String, value: String) -> Result<(), &'static str> {
        if self.inner.len() >= MAX_METADATA_ENTRIES && !self.inner.contains_key(&key) {
            return Err("metadata: max entries exceeded");
        }
        if key.len() > MAX_KEY_LEN {
            return Err("metadata: key too long");
        }
        if value.len() > MAX_VALUE_LEN {
            return Err("metadata: value too long");
        }
        self.inner.insert(key, value);
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.inner.get(key)
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.inner.iter()
    }
}

impl Default for BoundedMeta {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoIterator for BoundedMeta {
    type Item = (String, String);
    type IntoIter = std::collections::hash_map::IntoIter<String, String>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl FromIterator<(String, String)> for BoundedMeta {
    fn from_iter<T: IntoIterator<Item = (String, String)>>(iter: T) -> Self {
        let mut m = Self::new();
        for (k, v) in iter {
            let _ = m.insert(k, v);
        }
        m
    }
}

impl Serialize for BoundedMeta {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.inner.serialize(s)
    }
}

impl<'de> Deserialize<'de> for BoundedMeta {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let map: HashMap<String, String> = HashMap::deserialize(d)?;
        let mut m = Self::new();
        for (k, v) in map {
            m.insert(k, v).map_err(serde::de::Error::custom)?;
        }
        Ok(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_within_limits() {
        let mut m = BoundedMeta::new();
        assert!(m.insert("key".into(), "value".into()).is_ok());
        assert_eq!(m.get("key").unwrap(), "value");
    }

    #[test]
    fn rejects_too_many_entries() {
        let mut m = BoundedMeta::new();
        for i in 0..MAX_METADATA_ENTRIES {
            assert!(m.insert(format!("k{}", i), "v".into()).is_ok());
        }
        assert!(m.insert("one_too_many".into(), "v".into()).is_err());
    }

    #[test]
    fn allows_overwrite_within_limit() {
        let mut m = BoundedMeta::new();
        for i in 0..MAX_METADATA_ENTRIES {
            assert!(m.insert(format!("k{}", i), "v".into()).is_ok());
        }
        // Overwrite existing key is allowed
        assert!(m.insert("k0".into(), "new".into()).is_ok());
        assert_eq!(m.get("k0").unwrap(), "new");
    }

    #[test]
    fn rejects_long_key() {
        let mut m = BoundedMeta::new();
        let long_key = "x".repeat(MAX_KEY_LEN + 1);
        assert!(m.insert(long_key, "v".into()).is_err());
    }

    #[test]
    fn rejects_long_value() {
        let mut m = BoundedMeta::new();
        let long_val = "y".repeat(MAX_VALUE_LEN + 1);
        assert!(m.insert("key".into(), long_val).is_err());
    }

    #[test]
    fn json_roundtrip() {
        let mut m = BoundedMeta::new();
        m.insert("agent".into(), "portail".into()).unwrap();
        m.insert("version".into(), "0.6.0".into()).unwrap();
        let json = serde_json::to_string(&m).unwrap();
        let rt: BoundedMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.get("agent").unwrap(), "portail");
        assert_eq!(rt.len(), 2);
    }

    #[test]
    fn json_rejects_excessive_from_json() {
        let mut big = serde_json::Map::new();
        for i in 0..(MAX_METADATA_ENTRIES + 5) {
            big.insert(format!("k{}", i), serde_json::Value::String("v".into()));
        }
        let json = serde_json::to_string(&big).unwrap();
        let result: Result<BoundedMeta, _> = serde_json::from_str(&json);
        assert!(result.is_err());
    }
}
