# BOW — Best Objective World

> Local encrypted secret store for Portail.
> AES-256-GCM at rest, argon2id key derivation, CLI-first.

## Overview

BOW provides a local encrypted key-value store for secrets (API keys,
tokens, TLS certificates, passphrases) that Portail needs at runtime.
Secrets are encrypted before hitting SQLite and decrypted on read.
The master key never touches disk unencrypted.

```
┌────────────────────────────────────────────────────┐
│                   portail bow                      │
│                                                    │
│  CLI ──► BowStore ──► AES-256-GCM encrypt/decrypt │
│                │                                   │
│                ├──► SQLite (bundled, WAL mode)      │
│                └──► AuditLog (append-only)          │
│                                                    │
│  Master Key sources (priority order):              │
│    1. PORTAIL_MASTER_KEY env var                   │
│    2. ~/.config/portail/master.key (file)          │
│    3. Interactive prompt (stdin)                    │
└────────────────────────────────────────────────────┘
```

## SQL Schema

```sql
-- Encrypted secrets table
CREATE TABLE IF NOT EXISTS bow_secrets (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    key         TEXT    NOT NULL UNIQUE,             -- logical key (e.g. "openai/api_key")
    value       BLOB    NOT NULL,                    -- AES-256-GCM ciphertext || nonce || tag
    version     INTEGER NOT NULL DEFAULT 1,          -- rotation counter
    created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_bow_secrets_key ON bow_secrets(key);

-- Immutable audit log
CREATE TABLE IF NOT EXISTS bow_audit (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    secret_key  TEXT    NOT NULL,                    -- which secret was touched
    action      TEXT    NOT NULL,                    -- "get" | "set" | "delete" | "list"
    actor       TEXT    NOT NULL DEFAULT 'cli',      -- "cli" | "api" | "hook:<name>"
    ts          TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_bow_audit_key ON bow_audit(secret_key);
CREATE INDEX IF NOT EXISTS idx_bow_audit_ts  ON bow_audit(ts);
```

## Encryption Design

### Algorithm

- **Cipher**: AES-256-GCM (authenticated encryption with associated data)
- **Nonce**: 12 bytes, generated via `ring::aead` or `aes-gcm` crate
- **Tag**: 16 bytes (appended to ciphertext)
- **Layout**: `[12-byte nonce][ciphertext][16-byte GCM tag]`

### Key Derivation

```
master_passphrase
       │
       ▼
  argon2id(password, salt, m_cost=19456, t_cost=2, p=1, out_len=32)
       │
       ▼
  32-byte master key (AES-256)
```

- **Salt**: 16 bytes, stored per-database in a `bow_meta` table
- **Parameters**: m_cost=19456 KiB, t_cost=2, p=1 (OWASP 2024 recommendations)
- **Derivation crate**: `argon2` (RustCrypto)

### Master Key Resolution (priority order)

| Priority | Source | Details |
|----------|--------|---------|
| 1 | Env var | `PORTAIL_MASTER_KEY` — raw 32-byte hex string |
| 2 | File | `~/.config/portail/master.key` — 64-char hex, mode `0600` |
| 3 | Prompt | Interactive `rpassword::read_password()` — prompt: `BOW master key: ` |

If env var is set, file and prompt are skipped. If file exists and is
readable, prompt is skipped.

### Key Rotation

BOW supports per-key version bumps without re-encrypting all secrets:

1. `portail bow rotate <key>` increments `version` and re-encrypts the
   value with a fresh nonce.
2. Old ciphertext is overwritten in-place (single transaction).
3. Audit log records the rotation.

A full master-key rotation (`portail bow rekey`) decrypts every secret
with the old key and re-encrypts with the new key in a single
transaction. On failure, the old key remains valid.

## CLI Commands

```
portail bow set <key> <value>       # store a secret (create or update)
portail bow set <key>               # read value from stdin (hidden)
portail bow get <key>               # decrypt and print to stdout
portail bow list                    # list all keys (no values)
portail bow list --values           # list keys + masked values (last 4 chars)
portail bow delete <key>            # remove a secret (with confirmation)
portail bow delete <key> --force    # skip confirmation
portail bow rotate <key>            # re-encrypt with fresh nonce, bump version
portail bow rekey                   # re-encrypt all secrets with new master key
portail bow audit [--key <key>]     # show audit log (optionally filtered)
portail bow init                    # generate and store a new master key
```

### Flag conventions

- `--db <path>` — override database path (default: `~/.local/share/portail/secrets.db`)
- `--master-key <hex>` — override master key (for scripting)
- `--quiet` — suppress non-essential output
- `--yes` / `-y` — skip confirmation prompts

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Key not found |
| 3 | Decryption failed (wrong key) |
| 4 | Auth / permission error |

## Rust Module Layout

```
src/bow/
├── mod.rs              # BowStore struct, public API
├── crypto.rs           # encrypt/decrypt, argon2id KDF
├── key.rs              # master key resolution (env, file, prompt)
├── audit.rs            # audit log writes + queries
├── cli.rs              # CLI subcommand dispatch
└── migrations/
    └── 20260627000001_bow_init.sql
```

### Public API (`src/bow/mod.rs`)

```rust
pub struct BowStore {
    pool: sqlx::SqlitePool,
    key: Zeroizing<[u8; 32]>,
}

impl BowStore {
    /// Open or create the BOW database, run migrations.
    pub async fn open(db_path: &str) -> Result<Self, BowError>;

    /// Store or update a secret. Returns new version number.
    pub async fn set(&self, key: &str, value: &[u8]) -> Result<u32, BowError>;

    /// Decrypt and return a secret.
    pub async fn get(&self, key: &str) -> Result<Vec<u8>, BowError>;

    /// List all keys (no decryption).
    pub async fn list(&self) -> Result<Vec<SecretMeta>, BowError>;

    /// Delete a secret.
    pub async fn delete(&self, key: &str) -> Result<(), BowError>;

    /// Re-encrypt a secret with a fresh nonce.
    pub async fn rotate(&self, key: &str) -> Result<u32, BowError>;

    /// Re-encrypt all secrets with a new master key.
    pub async fn rekey(&self, new_key: &[u8; 32]) -> Result<(), BowError>;

    /// Query audit log.
    pub async fn audit(&self, key_filter: Option<&str>) -> Result<Vec<AuditEntry>, BowError>;
}
```

### Error type

```rust
#[derive(Debug, thiserror::Error)]
pub enum BowError {
    #[error("key not found: {0}")]
    NotFound(String),

    #[error("decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("no master key available: {0}")]
    NoMasterKey(String),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
```

## Security Considerations

### Memory safety

- Master key held in `Zeroizing<[u8; 32]>` (from `zeroize` crate).
  Dropping the struct zeroizes the memory.
- Intermediate buffers (ciphertext, plaintext) are wrapped in
  `Zeroizing<Vec<u8>>`.
- `#![forbid(unsafe_code)]` is already enforced project-wide.

### Key derivation

- argon2id with OWASP 2024 parameters. Not tunable per-user — this is
  a single-operator secret store, not a multi-tenant system.
- Salt is generated once during `bow init` and stored in `bow_meta`.
  Never re-derived.

### File permissions

- `master.key` is created with mode `0600` (owner-only read/write).
- `secrets.db` is created with mode `0600`.
- If permissions are wrong, `bow init` warns and refuses to proceed.

### Audit log

- Append-only. No UPDATE or DELETE on `bow_audit`.
- Every `get`, `set`, `delete`, `rotate` writes an entry.
- Actor field populated from process context (env `BOW_ACTOR`, or `cli`).
- Audit entries include ISO-8601 timestamps.

### At-rest encryption

- Every value is encrypted with a unique 12-byte nonce.
- GCM authentication tag detects tampering.
- Database file can be backed up safely — it's encrypted at the row level.

### No network exposure

- BOW is CLI-only and local-only. No HTTP endpoints.
- Secrets never leave the machine. No telemetry, no sync, no cloud.

## Integration with Portail

### Config (`src/config.rs`)

Add to `Config`:

```toml
[bow]
enabled = true
db_path = "~/.local/share/portail/secrets.db"
# master_key is NOT in config — sourced from env/file/prompt only
```

### Runtime secret access

Other Portail modules can access BOW at runtime:

```rust
// In AppState or during startup
let bow = BowStore::open(&config.bow.db_path).await?;
let openai_key = bow.get("openai/api_key").await?;
```

### Environment variable forwarding

Gateway, MCP sidecar, and hooks can reference BOW keys via a special
syntax in config values:

```toml
[gateway.providers.openai]
api_key = "bow:openai/api_key"   # resolved at startup from BOW
```

The `bow:` prefix triggers BOW lookup. The resolved value is held in
memory (Zeroizing) for the lifetime of the process.

### Migration path

BOW is a new module — no migration from existing systems needed. It
ships as an opt-in feature behind the `bow` cargo feature flag until
stabilized.

```toml
[features]
bow = []
```

## Dependencies (new)

| Crate | Version | Purpose |
|-------|---------|---------|
| `aes-gcm` | 0.10 | AES-256-GCM encrypt/decrypt |
| `argon2` | 0.5 | argon2id key derivation |
| `zeroize` | 1.8 | Zeroing memory on drop |
| `rpassword` | 7 | Hidden terminal input |
| `thiserror` | 2 | Error derive |

All crates are RustCrypto/zeroize ecosystem — audited, no C deps.

## Open Questions

1. **Separate DB or same DB?** — Default is a separate `secrets.db` to
   avoid accidental leaks from event-store backups. Could be configurable
   to use the main Portail DB.
2. **Secret namespaces?** — `key` is currently a flat string with `/`
   as visual separator only. Could add a `namespace` column later.
3. **Expiry/TTL?** — Not in v1. Secrets persist until deleted. Could
   add `expires_at` column for rotating tokens.
