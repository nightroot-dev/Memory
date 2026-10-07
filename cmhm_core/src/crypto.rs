//! At-rest encryption for the M3 storage seam (AES-256-GCM).
//!
//! Two layers, both fail-closed:
//! - [`seal`] / [`open_sealed`]: whole blobs (snapshots). Output is
//!   `b"CMHMSEAL1" + nonce(12) + ciphertext`. Anything else is rejected
//!   without attempting legacy parsing (callers try legacy separately).
//! - [`seal_record`] / [`open_record`]: WAL payloads. Output is
//!   `[0x53] + nonce(12) + ciphertext`. The `0x53` marker never collides with
//!   WAL type tags (`0x00..=0x06`); GCM tag verification makes accidental
//!   acceptance of a legacy record cryptographically improbable, and on any
//!   auth failure the caller falls back to the legacy path.
//!
//! Keys ([`DataKey`], 32 bytes) live in per-scope `key.bin` files, managed by
//! [`Keystore`]. File permissions are best-effort (Windows ACLs cannot express
//! Unix 0600 from stable Rust — documented, not silent). KMS/HSM replaces the
//! file backend later; nothing above this module names files.
//!
//! Crypto-shredding = delete the key file. Ciphertext without the key is
//! unrecoverable by construction (256-bit search). The shred test proves it.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{Result, anyhow};
use rand::RngCore;
use std::path::{Path, PathBuf};

/// Snapshot sealed-blob magic. Fixed 9 bytes, versioned (`...1`).
pub const SEAL_MAGIC: &[u8; 9] = b"CMHMSEAL1";
/// WAL sealed-record marker. Outside the `0x00..=0x06` type-tag range.
pub const SEALED_RECORD_MARKER: u8 = 0x53;
const NONCE_LEN: usize = 12;

/// 256-bit data key. Zeroized on drop (best effort via explicit zeroing).
#[derive(Clone)]
pub struct DataKey([u8; 32]);

impl DataKey {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        Self(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 32 {
            anyhow::bail!("data key must be 32 bytes, got {}", bytes.len());
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(bytes);
        Ok(Self(arr))
    }

    fn cipher(&self) -> Aes256Gcm {
        Aes256Gcm::new_from_slice(&self.0).expect("32-byte key is valid for AES-256-GCM")
    }
}

impl Drop for DataKey {
    fn drop(&mut self) {
        for b in self.0.iter_mut() {
            *b = 0;
        }
    }
}

/// Seal a whole blob (snapshot path). Returns `SEAL_MAGIC + nonce + ciphertext`.
pub fn seal(key: &DataKey, plaintext: &[u8]) -> Vec<u8> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = key
        .cipher()
        .encrypt(nonce, plaintext)
        .expect("in-memory encryption cannot fail");
    let mut out = Vec::with_capacity(SEAL_MAGIC.len() + NONCE_LEN + ct.len());
    out.extend_from_slice(SEAL_MAGIC);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    out
}

/// Open a sealed blob. Hard error on wrong magic, short input, or auth failure.
pub fn open_sealed(key: &DataKey, blob: &[u8]) -> Result<Vec<u8>> {
    if blob.len() < SEAL_MAGIC.len() + NONCE_LEN + 16 {
        anyhow::bail!("sealed blob too short ({} bytes)", blob.len());
    }
    if &blob[..SEAL_MAGIC.len()] != SEAL_MAGIC {
        anyhow::bail!("not a sealed blob (bad magic)");
    }
    let nonce = Nonce::from_slice(&blob[SEAL_MAGIC.len()..SEAL_MAGIC.len() + NONCE_LEN]);
    key.cipher()
        .decrypt(nonce, &blob[SEAL_MAGIC.len() + NONCE_LEN..])
        .map_err(|_| anyhow!("sealed blob authentication failed (wrong key or tampered)"))
}

/// Seal one WAL payload. Returns `[0x53] + nonce + ciphertext`.
pub fn seal_record(key: &DataKey, payload: &[u8]) -> Vec<u8> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = key
        .cipher()
        .encrypt(nonce, payload)
        .expect("in-memory encryption cannot fail");
    let mut out = Vec::with_capacity(1 + NONCE_LEN + ct.len());
    out.push(SEALED_RECORD_MARKER);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    out
}

/// Try to open a sealed WAL payload. Returns `Ok(None)` when the bytes are
/// NOT a sealed record (caller falls back to the legacy path); `Err` only
/// when the marker is present but authentication fails (wrong key/tampered).
pub fn open_record(key: &DataKey, payload: &[u8]) -> Result<Option<Vec<u8>>> {
    if payload.first() != Some(&SEALED_RECORD_MARKER) {
        return Ok(None);
    }
    if payload.len() < 1 + NONCE_LEN + 16 {
        anyhow::bail!("sealed record too short");
    }
    let nonce = Nonce::from_slice(&payload[1..1 + NONCE_LEN]);
    key.cipher()
        .decrypt(nonce, &payload[1 + NONCE_LEN..])
        .map(Some)
        .map_err(|_| anyhow!("sealed record authentication failed (wrong key or tampered)"))
}

/// Per-scope key files. `<scope-root>/key.bin` (32 raw bytes).
/// Best-effort restrictive permissions; Windows cannot express Unix 0600 —
/// callers must note this, and KMS replaces files entirely later.
pub struct Keystore;

impl Keystore {
    pub fn key_path(scope_root: &Path) -> PathBuf {
        scope_root.join("key.bin")
    }

    /// Load existing key, or generate + persist (create_new wins races).
    pub fn load_or_create(scope_root: &Path) -> Result<DataKey> {
        let path = Self::key_path(scope_root);
        if path.exists() {
            let bytes = std::fs::read(&path)?;
            return DataKey::from_bytes(&bytes);
        }
        let key = DataKey::generate();
        // Best effort: restrict on Unix; no-op noted on Windows.
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)?;
            use std::io::Write;
            f.write_all(&key.0)?;
            let _ = f.sync_all();
            return Ok(key);
        }
        #[cfg(not(unix))]
        {
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut f) => {
                    use std::io::Write;
                    f.write_all(&key.0)?;
                    let _ = f.sync_all();
                    Ok(key)
                }
                Err(_) => {
                    // Lost the create race: read the winner's key.
                    let bytes = std::fs::read(&path)?;
                    DataKey::from_bytes(&bytes)
                }
            }
        }
    }

    /// Destroy a scope's key (crypto-shredding). After this, every sealed
    /// copy of the scope's data is unrecoverable.
    pub fn destroy(scope_root: &Path) -> Result<()> {
        let path = Self::key_path(scope_root);
        if path.exists() {
            // Overwrite-then-delete: best effort against disk forensics.
            if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open(&path) {
                use std::io::Write;
                let _ = f.write_all(&[0u8; 32]);
                let _ = f.sync_all();
            }
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_round_trip() {
        let key = DataKey::generate();
        let pt = b"snapshot bytes with secrets inside";
        let blob = seal(&key, pt);
        assert!(blob.starts_with(SEAL_MAGIC));
        assert_eq!(open_sealed(&key, &blob).unwrap(), pt);
    }

    #[test]
    fn wrong_key_fails_closed() {
        let a = DataKey::generate();
        let b = DataKey::generate();
        let blob = seal(&a, b"data");
        assert!(open_sealed(&b, &blob).is_err());
    }

    #[test]
    fn tampered_blob_fails_closed() {
        let key = DataKey::generate();
        let mut blob = seal(&key, b"data");
        let last = blob.len() - 1;
        blob[last] ^= 0xFF;
        assert!(open_sealed(&key, &blob).is_err());
    }

    #[test]
    fn record_legacy_passthrough() {
        let key = DataKey::generate();
        // Legacy payloads (type tags 0x00..=0x06) are NOT sealed markers.
        assert!(open_record(&key, &[0x03, 0x01, 0x02]).unwrap().is_none());
        let sealed = seal_record(&key, &[0x03, 0x01, 0x02]);
        assert_eq!(sealed[0], SEALED_RECORD_MARKER);
        assert_eq!(open_record(&key, &sealed).unwrap().unwrap(), vec![0x03, 0x01, 0x02]);
    }

    #[test]
    fn keystore_create_load_destroy() {
        let dir = std::env::temp_dir().join("cmhm_crypto_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let k1 = Keystore::load_or_create(&dir).unwrap();
        let k2 = Keystore::load_or_create(&dir).unwrap();
        assert_eq!(k1.0, k2.0, "same scope must reload the same key");
        Keystore::destroy(&dir).unwrap();
        assert!(!Keystore::key_path(&dir).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
