use std::fs::{File, OpenOptions};
use std::io::{Write, Read, Seek, SeekFrom, BufWriter, BufReader};
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use serde::{Deserialize, Serialize};
use anyhow::{Result, anyhow};

use crate::crypto::{self, DataKey};

// ─── WAL constants ────────────────────────────────────────────────────────────

/// Magic bytes written as the first 4 bytes of every new WAL file.
/// Lets `read_wal` detect misalignment and bail cleanly instead of reading garbage.
const WAL_MAGIC: &[u8; 4] = b"CWAL";

/// Maximum sane record size. Protects against corrupted length fields.
const WAL_MAX_RECORD_BYTES: usize = 64 * 1024 * 1024; // 64 MiB

/// Number of snapshot generations to keep.
/// Keep 3: current + 2 fallbacks. Never delete old until new is durable.
const SNAPSHOT_GENERATIONS: u32 = 3;

/// Current snapshot schema version.
/// **Bump this whenever `CoreState` gains, loses, or reorders fields.**
///
/// Version history:
///   1 = pre-versioned (no envelope; feedback_seen: HashSet<String>)
///   2 = SnapshotEnvelope added; feedback_seen: HashSet<u64> (FNV-1a); WAL type tags
///
/// On version mismatch, `read_snapshot` returns `SnapshotLoadError::VersionMismatch`.
/// The engine discards the snapshot and rebuilds from WAL — it does NOT treat this as
/// corruption (no quarantine, no fsck). A genuine bit-flip produces a bincode error,
/// which becomes `SnapshotLoadError::Corrupt`.
/// Version history:
///   1 = pre-versioned (no envelope; `feedback_seen: HashSet<String>`)
///   2 = `SnapshotEnvelope`; `feedback_seen: HashSet<u64>` (FNV-1a); WAL type tags
///   3 = `Node` gains `authority: f32`, `provenance: String`, `superseded_by: Option<u64>`.
///       `WalLearnEntryV2` written as `WAL_TYPE_LEARN_V3` (type `0x03`) for verified post-normalization writes.
///       Legacy `WAL_TYPE_LEARN_V2` (type `0x02`) records replayed conservatively with `CallerRole::Agent`.
///       Legacy `WalLearnEntry` (type `0x00`) replayed with neutral defaults.
///
/// Migration from v2 → v3
/// ───────────────────────
/// Snapshot: `VersionMismatch` returned on load. With `allow_incompatible_snapshot_reset = true`
/// the engine quarantines the snapshot and replays the WAL. Old WAL type-`0x00` entries are
/// replayed through `process_learn` with `authority=0.5, provenance="", supersedes=None`
/// (safe defaults). Data is recovered from the WAL. A new v3 snapshot is written at the
/// next periodic checkpoint or clean shutdown.
/// Context identity (UUID in the context registry) is preserved across migration.
pub const SNAPSHOT_VERSION: u32 = 3;


/// Thin envelope that tags snapshot bytes with a schema version.
/// Bincode-serialized as the outer container in every snapshot file.
///
/// ## Why not `#[serde(default)]` on `CoreState`?
/// Bincode is non-self-describing. Deserializing a struct with a new field into bytes
/// from an old version reads the next bytes as the new field — silent misparse.
/// A version envelope makes the format self-describing at the outer level:
/// we validate the version before handing the inner bytes to bincode.
#[derive(serde::Serialize, serde::Deserialize)]
struct SnapshotEnvelope {
    version: u32,
    payload: Vec<u8>, // bincode-serialized CoreState
}

/// Error returned by `read_snapshot` with a cause classification.
/// Callers MUST distinguish `VersionMismatch` from `Corrupt`:
/// - `VersionMismatch` → attempt snapshot migration or discard snapshot, rebuild from WAL
/// - `Corrupt`         → potential data loss; log at ERROR, consider quarantine
#[derive(Debug)]
pub enum SnapshotLoadError {
    /// Snapshot exists but was written by a different schema version.
    /// Carries the inner payload so migration engines can upgrade without re-reading disk.
    VersionMismatch { found: u32, expected: u32, payload: Vec<u8> },
    /// Snapshot is present but bytes are unreadable (I/O, corrupt envelope, etc.).
    Corrupt(anyhow::Error),
    /// No snapshot file exists at any generation.
    NotFound,
}

impl std::fmt::Display for SnapshotLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VersionMismatch { found, expected, .. } =>
                write!(f, "snapshot version mismatch (found={found}, expected={expected})"),
            Self::Corrupt(e) => write!(f, "snapshot corrupt: {e}"),
            Self::NotFound   => write!(f, "no snapshot found"),
        }
    }
}


// ─── Storage seam (M3): per-scope encryption without touching the engine ───
//
// `engine.rs` calls the same `WalWriter::open/append`, `read_wal`,
// `write_snapshot`, `read_snapshot` as before. Encryption is decided HERE by
// looking up a data key for the file's scope directory:
//
// - Key present  → new writes are sealed (WAL records individually, snapshots
//   whole-file). Reads try sealed-first, then legacy (mixed generations work).
// - Key absent   → legacy plaintext behavior, byte-identical to before.
// - Sealed bytes + no key → fail-closed `Corrupt("encrypted, no key")`, never
//   misclassified as a version issue.
//
// Keys are registered by the scope layer (`scope.rs` adopt path) via
// `register_scope_key`. One global registry, keyed by scope root directory;
// lookup walks up at most two levels (file → scope root).
static KEY_REGISTRY: std::sync::LazyLock<RwLock<std::collections::HashMap<PathBuf, DataKey>>> =
    std::sync::LazyLock::new(|| RwLock::new(std::collections::HashMap::new()));

/// Register a scope's data key. Called once per scope load (idempotent).
pub fn register_scope_key(scope_root: &Path, key: DataKey) {
    if let Ok(mut reg) = KEY_REGISTRY.write() {
        reg.insert(scope_root.to_path_buf(), key);
    }
}

/// Forget a scope's key (e.g. after crypto-shredding). Reads fail closed after.
pub fn unregister_scope_key(scope_root: &Path) {
    if let Ok(mut reg) = KEY_REGISTRY.write() {
        reg.remove(scope_root);
    }
}

fn key_for_file(path: &Path) -> Option<DataKey> {
    let reg = KEY_REGISTRY.read().ok()?;
    // Exact dir, then parent (covers `<root>/wal.dat`, `<root>/snapshots/x`).
    let mut cur: Option<&Path> = path.parent();
    for _ in 0..3 {
        let dir = cur?;
        if let Some(k) = reg.get(dir) {
            return Some(k.clone());
        }
        cur = dir.parent();
    }
    None
}

// ─── Write-Ahead Log ─────────────────────────────────────────────────────────

/// Write-Ahead Log writer.
///
/// ## On-disk format
/// File header (written once on creation): `[CWAL: 4 bytes]`
/// Per record: `[len: u32 LE][payload: len bytes][crc32: u32 LE]`
///
/// ## Durability guarantee
/// After every `append()`: `flush()` + `sync_data()` — data reaches the device.
/// This is `O_DSYNC` semantics without requiring that flag at open time.
pub struct WalWriter {
    file: BufWriter<File>,
    pub seq: u64,
    /// Seals record payloads when the scope registered a key. `None` keeps
    /// the legacy plaintext format byte-identical.
    key: Option<DataKey>,
}

impl WalWriter {
    /// Open (or create) a WAL file.
    ///
    /// If the file is newly created (size == 0), writes the 4-byte magic header.
    /// If the file already exists, assumes magic was written on creation.
    /// Picks up the scope key (if registered for this path) for new appends;
    /// pre-existing plaintext records keep replaying unchanged.
    pub fn open(path: &PathBuf) -> Result<Self> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;

        // Write magic header only on a brand-new (empty) file.
        // Appending to an existing file: magic was already written.
        let len = file.seek(SeekFrom::End(0))?;
        if len == 0 {
            file.write_all(WAL_MAGIC)?;
            file.flush()?;
            // sync_data on new file — ensures magic reaches disk before any records
            file.sync_data()?;
        }

        let key = key_for_file(path);
        Ok(Self { file: BufWriter::new(file), seq: 0, key })
    }

    /// Append one serialized record to the WAL.
    ///
    /// Writes: `[len: u32 LE][payload][crc32: u32 LE]`
    /// Then flushes the buffer and calls `sync_data()` to guarantee durability.
    ///
    /// When a scope key is present, `payload` is sealed first (record becomes
    /// `[0x53][nonce][ciphertext]`); `len`/`crc` still frame the sealed bytes,
    /// so torn-tail and CRC handling are unchanged.
    pub fn append(&mut self, cmd_bytes: &[u8]) -> Result<()> {
        self.seq += 1;
        let sealed;
        let payload: &[u8] = match &self.key {
            Some(k) => {
                sealed = crypto::seal_record(k, cmd_bytes);
                &sealed
            }
            None => cmd_bytes,
        };
        let len = payload.len() as u32;
        let crc = crc32(payload);

        self.file.write_all(&len.to_le_bytes())?;
        self.file.write_all(payload)?;
        self.file.write_all(&crc.to_le_bytes())?;
        self.file.flush()?;

        // Flush OS page cache to physical media.
        // `sync_data` skips metadata (mtime etc) — cheaper than `sync_all`,
        // sufficient for WAL durability.
        self.file.get_ref().sync_data()?;

        Ok(())
    }
}

// ─── WAL reader ──────────────────────────────────────────────────────────────

/// WAL entry for replay.
#[derive(Debug, Serialize, Deserialize)]
pub struct WalEntry {
    pub payload: Vec<u8>,
}

/// Read all valid WAL entries from a file.
///
/// ## Crash-safety contract
/// - Stops at the first CRC mismatch, short read, or implausible length.
/// - A torn tail (partial last write after a crash) is silently truncated — not an error.
/// - If the file doesn't exist, returns an empty list (clean start).
/// - The caller is responsible for replaying and re-deriving state from entries.
///
/// ## Magic handling
/// - If the file begins with `CWAL`, skip the 4-byte header before reading records.
/// - If no magic (legacy file created before this format), read records from offset 0.
pub fn read_wal(path: &PathBuf) -> Result<Vec<WalEntry>> {
    let file = match File::open(path) {
        Ok(f)  => f,
        Err(_) => return Ok(vec![]),  // file not found = clean start
    };
    let mut reader = BufReader::new(file);
    let mut entries = Vec::new();
    // Sealed records decrypt with the scope key when one is registered for
    // this path; legacy plaintext records pass through untouched either way.
    let key = key_for_file(path);

    // Detect and skip optional file-level magic header
    let mut magic_buf = [0u8; 4];
    match reader.read_exact(&mut magic_buf) {
        Ok(()) if &magic_buf == WAL_MAGIC => {
            // Magic present — records start after these 4 bytes. Already consumed.
        }
        Ok(()) => {
            // No magic (legacy file) — these 4 bytes are the start of the first record.
            // Seek back to reprocess them as a length prefix.
            // We cannot un-read from a BufReader, so handle inline:
            let len = u32::from_le_bytes(magic_buf) as usize;
            if len == 0 || len > WAL_MAX_RECORD_BYTES {
                return Ok(entries); // corrupted first record
            }
            if let Some(entry) = read_one_record(&mut reader, len, key.as_ref())? {
                entries.push(entry);
            } else {
                return Ok(entries);
            }
        }
        Err(_) => return Ok(entries), // empty file
    }

    // Read remaining records
    loop {
        let mut len_buf = [0u8; 4];
        if reader.read_exact(&mut len_buf).is_err() {
            break; // clean EOF (or torn tail — both treated as end of valid data)
        }
        let len = u32::from_le_bytes(len_buf) as usize;
        if len == 0 || len > WAL_MAX_RECORD_BYTES {
            tracing::warn!("WAL: implausible record length {len} — stopping replay");
            break;
        }
        match read_one_record(&mut reader, len, key.as_ref())? {
            Some(entry) => entries.push(entry),
            None        => break, // CRC mismatch, auth failure, or short read — stop here
        }
    }

    tracing::info!("WAL replay: {} valid entries", entries.len());
    Ok(entries)
}

/// Read exactly one record (payload + CRC) given the length field was already read.
/// Returns `None` on any read error or CRC mismatch — caller stops replay.
///
/// When `key` is present, sealed records (`0x53` marker) are opened first;
/// an authentication failure stops replay with a warning (same posture as a
/// CRC mismatch — the tail from here on is untrusted). Legacy records pass
/// through untouched, so mixed plaintext/sealed WALs replay seamlessly.
fn read_one_record<R: Read>(reader: &mut R, len: usize, key: Option<&DataKey>) -> Result<Option<WalEntry>> {
    let mut payload = vec![0u8; len];
    if reader.read_exact(&mut payload).is_err() {
        return Ok(None); // torn payload
    }
    let mut crc_buf = [0u8; 4];
    if reader.read_exact(&mut crc_buf).is_err() {
        return Ok(None); // torn CRC
    }
    let stored_crc   = u32::from_le_bytes(crc_buf);
    let computed_crc = crc32(&payload);
    if stored_crc != computed_crc {
        tracing::warn!("WAL: CRC mismatch (stored={stored_crc:#010x} computed={computed_crc:#010x}) — stopping replay");
        return Ok(None);
    }
    if let Some(k) = key {
        match crypto::open_record(k, &payload)? {
            Some(inner) => return Ok(Some(WalEntry { payload: inner })),
            None => {} // legacy plaintext record — fall through
        }
    } else if payload.first() == Some(&crypto::SEALED_RECORD_MARKER) {
        // Sealed bytes but no key registered: fail closed loudly. This is NOT
        // corruption or a version issue — the key is simply absent.
        tracing::error!("WAL: sealed record with no registered key — stopping replay (destroyed/missing key?)");
        return Ok(None);
    }
    Ok(Some(WalEntry { payload }))
}

// ─── CRC32 (Castagnoli / CRC32C) ─────────────────────────────────────────────

/// CRC32 with the Castagnoli polynomial (0x1EDC6F41, reflected 0xEDB88320).
///
/// P(undetected random corruption per record) ≈ 2⁻³² ≈ 2.3×10⁻¹⁰.
/// Detects all 1-3 bit errors and all burst errors ≤ 32 bits.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

// ─── Snapshot (atomic, N-generation) ─────────────────────────────────────────

/// Write a versioned bincode snapshot atomically, keeping `SNAPSHOT_GENERATIONS` copies.
///
/// ## On-disk format
/// `[bincode(SnapshotEnvelope { version: SNAPSHOT_VERSION, payload: bincode(T) })]`
///
/// ## Write sequence (prevents partial-write corruption)
/// 1. Serialize T → payload bytes
/// 2. Wrap in `SnapshotEnvelope` → envelope bytes
/// 3. Write envelope to `.tmp`, `sync_all()`
/// 4. Rotate generations: `.2 ← .1 ← current ← .tmp`
/// 5. `sync_all()` on parent directory (POSIX; no-op on Windows)
///
/// ## Generation rotation
/// On every save: current → `.1`, `.1` → `.2`, `.2` is deleted.
/// If the current save fails, the caller still has `.1` and `.2` as fallbacks.
pub fn write_snapshot<T: serde::Serialize>(state: &T, path: &PathBuf) -> Result<()> {
    let payload = bincode::serialize(state)?;
    let envelope = SnapshotEnvelope { version: SNAPSHOT_VERSION, payload };
    let mut bytes = bincode::serialize(&envelope)?;
    // Seal whole-file when the scope registered a key. The sealed form is
    // `CMHMSEAL1 + nonce + ciphertext`; readers detect it by magic.
    if let Some(key) = key_for_file(path) {
        bytes = crypto::seal(&key, &bytes);
    }
    let tmp_path = path.with_extension("bin.tmp");

    // Step 1+2: Write to .tmp and fsync it
    {
        let mut tmp_file = File::create(&tmp_path)?;
        tmp_file.write_all(&bytes)?;
        tmp_file.flush()?;
        tmp_file.sync_all()?; // full fsync — data + metadata
    }

    // Step 3: Rotate existing generations before the rename
    rotate_generations(path)?;

    // Step 4: Atomic rename .tmp → current
    std::fs::rename(&tmp_path, path)?;

    // Step 5: fsync parent directory (POSIX: makes the rename durable)
    sync_dir(path);

    Ok(())
}

/// Deserialize a versioned snapshot.
///
/// ## Error classification
/// - `SnapshotLoadError::NotFound`        — no file at any generation
/// - `SnapshotLoadError::VersionMismatch` — file readable, version tag wrong → rebuild from WAL
/// - `SnapshotLoadError::Corrupt`         — bytes unreadable → log ERROR, consider fsck
///
/// Falls back through generations (`.1`, `.2`) for `Corrupt` only.
/// A `VersionMismatch` on the primary generation is returned immediately
/// (older generations have the same or older version — no point trying them).
pub fn read_snapshot<T: for<'de> serde::Deserialize<'de>>(path: &PathBuf)
    -> std::result::Result<T, SnapshotLoadError>
{
    let candidates = [
        path.clone(),
        generation_path(path, 1),
        generation_path(path, 2),
    ];

    let mut found_any = false;
    for candidate in &candidates {
        if !candidate.exists() { continue; }
        found_any = true;

        let mut bytes = match std::fs::read(candidate) {
            Err(e) => {
                tracing::warn!("Snapshot {:?} unreadable: {e}", candidate);
                continue; // try next generation
            }
            Ok(b) => b,
        };

        // Sealed snapshot? Open with the registered key, or fail closed when
        // the key is absent (encrypted data without a key is NOT corruption
        // and NOT a version issue — say exactly that).
        if bytes.starts_with(crypto::SEAL_MAGIC) {
            match key_for_file(candidate) {
                Some(key) => match crypto::open_sealed(&key, &bytes) {
                    Ok(inner) => bytes = inner,
                    Err(e) => {
                        tracing::error!("Snapshot {:?}: sealed but cannot open ({e})", candidate);
                        continue; // try next generation
                    }
                },
                None => {
                    tracing::error!(
                        "Snapshot {:?}: encrypted but no key registered (destroyed key?) — refusing to guess",
                        candidate
                    );
                    continue; // try next generation
                }
            }
        }

        // Try to parse the outer envelope first.
        let envelope = match bincode::deserialize::<SnapshotEnvelope>(&bytes) {
            Ok(env) => env,
            Err(_) => {
                // Could not deserialize envelope — either corrupt or pre-versioned (v1).
                // Treat as VersionMismatch (version=1 implied) so the caller rebuilds from WAL.
                tracing::warn!(
                    "Snapshot {:?}: no version envelope (pre-v{SNAPSHOT_VERSION} format) — \
                     discarding, will rebuild from WAL", candidate
                );
                return Err(SnapshotLoadError::VersionMismatch {
                    found: 1,
                    expected: SNAPSHOT_VERSION,
                    payload: bytes,
                });
            }
        };

        if envelope.version != SNAPSHOT_VERSION {
            tracing::warn!(
                "Snapshot {:?}: version {} ≠ expected {} — attempting migration or fallback",
                candidate, envelope.version, SNAPSHOT_VERSION
            );
            return Err(SnapshotLoadError::VersionMismatch {
                found:    envelope.version,
                expected: SNAPSHOT_VERSION,
                payload:  envelope.payload,
            });
        }


        // Version matches: deserialize inner payload.
        match bincode::deserialize::<T>(&envelope.payload) {
            Ok(state) => {
                tracing::info!("Snapshot v{} loaded from {:?}", SNAPSHOT_VERSION, candidate);
                return Ok(state);
            }
            Err(e) => {
                tracing::warn!("Snapshot {:?} payload corrupt ({e}), trying next generation", candidate);
                // Fall through to next generation.
            }
        }
    }

    if found_any {
        Err(SnapshotLoadError::Corrupt(anyhow!("all snapshot generations failed to deserialize")))
    } else {
        Err(SnapshotLoadError::NotFound)
    }
}

// ─── Generation helpers ───────────────────────────────────────────────────────

/// Rotate snapshot generations: `current → .1`, `.1 → .2`, delete `.2`.
fn rotate_generations(path: &PathBuf) -> Result<()> {
    // Delete oldest generation first
    let oldest = generation_path(path, SNAPSHOT_GENERATIONS - 1);
    let _ = std::fs::remove_file(&oldest); // ok if not present

    // Shift: .1 → .2, current → .1 (in reverse order to avoid overwrites)
    for gen in (1..SNAPSHOT_GENERATIONS).rev() {
        let from = generation_path(path, gen - 1);
        let to   = generation_path(path, gen);
        if from.exists() {
            std::fs::rename(&from, &to)?;
        }
    }
    Ok(())
}

/// Build the path for snapshot generation N.
/// N=0 → `path` (unchanged), N=1 → `path.1`, N=2 → `path.2`.
fn generation_path(path: &PathBuf, n: u32) -> PathBuf {
    if n == 0 {
        path.clone()
    } else {
        let ext = path.extension()
            .map(|e| format!("{}.{n}", e.to_string_lossy()))
            .unwrap_or_else(|| n.to_string());
        path.with_extension(ext)
    }
}

/// Fsync the parent directory of `path` to make any pending renames durable.
///
/// On POSIX, a rename is not durable until the directory is fsynced.
/// On Windows, NTFS journals renames — we skip and warn.
fn sync_dir(path: &Path) {
    let parent = path.parent().unwrap_or(Path::new("."));
    #[cfg(unix)]
    {
        match File::open(parent) {
            Ok(dir) => {
                if let Err(e) = dir.sync_all() {
                    tracing::warn!("fsync({:?}) failed: {e} — rename durability not guaranteed", parent);
                }
            }
            Err(e) => {
                tracing::warn!("open({:?}) for fsync failed: {e}", parent);
            }
        }
    }
    #[cfg(not(unix))]
    {
        tracing::trace!("Directory fsync skipped on non-POSIX platform (parent: {:?})", parent);
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cmhm_{name}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // --- CRC32 ------------------------------------------------------------------

    #[test]
    fn crc32_deterministic() {
        let a = crc32(b"hello cmhm");
        let b = crc32(b"hello cmhm");
        assert_eq!(a, b);
        assert_ne!(a, crc32(b"different"));
    }

    #[test]
    fn crc32_detects_single_bit_flip() {
        let original = b"test payload for cmhm wal";
        let c_orig = crc32(original);
        let mut flipped = original.to_vec();
        flipped[3] ^= 0x01;
        let c_flip = crc32(&flipped);
        assert_ne!(c_orig, c_flip, "CRC must detect a single-bit flip");
    }

    // --- WAL magic + round-trip -------------------------------------------------

    #[test]
    fn wal_writes_magic_on_new_file() -> Result<()> {
        let dir  = tmp_dir("wal_magic");
        let path = dir.join("test.wal");
        let _ = std::fs::remove_file(&path);

        let mut w = WalWriter::open(&path)?;
        w.append(b"hello")?;
        drop(w);

        let raw = std::fs::read(&path)?;
        assert_eq!(&raw[..4], WAL_MAGIC, "First 4 bytes must be WAL magic");

        std::fs::remove_dir_all(&dir).ok();
        Ok(())
    }

    #[test]
    fn wal_round_trip_with_magic() -> Result<()> {
        let dir  = tmp_dir("wal_rt");
        let path = dir.join("test.wal");
        let _ = std::fs::remove_file(&path);

        {
            let mut w = WalWriter::open(&path)?;
            w.append(b"alpha")?;
            w.append(b"beta")?;
            w.append(b"gamma")?;
        }

        let entries = read_wal(&path)?;
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].payload, b"alpha");
        assert_eq!(entries[1].payload, b"beta");
        assert_eq!(entries[2].payload, b"gamma");

        std::fs::remove_dir_all(&dir).ok();
        Ok(())
    }

    #[test]
    fn wal_append_to_existing_no_double_magic() -> Result<()> {
        let dir  = tmp_dir("wal_append");
        let path = dir.join("test.wal");
        let _ = std::fs::remove_file(&path);

        // First session
        { let mut w = WalWriter::open(&path)?; w.append(b"one")?; }
        // Second session (must NOT write magic again)
        { let mut w = WalWriter::open(&path)?; w.append(b"two")?; }

        let entries = read_wal(&path)?;
        assert_eq!(entries.len(), 2, "Both sessions' entries must be readable");
        assert_eq!(entries[0].payload, b"one");
        assert_eq!(entries[1].payload, b"two");

        std::fs::remove_dir_all(&dir).ok();
        Ok(())
    }

    #[test]
    fn wal_stops_at_crc_mismatch() -> Result<()> {
        let dir  = tmp_dir("wal_crc");
        let path = dir.join("corrupt.wal");
        let _ = std::fs::remove_file(&path);

        {
            let mut w = WalWriter::open(&path)?;
            w.append(b"good_one")?;   // entry 0
            w.append(b"good_two")?;   // entry 1 — we will corrupt this
            w.append(b"good_three")?; // entry 2 — should NOT be read
        }

        // Layout after magic (4):
        //   entry 0: [len:4][payload:8][crc:4] = 16 bytes  → offsets 4..20
        //   entry 1: [len:4][payload:8][crc:4] = 16 bytes  → offsets 20..36
        //     CRC of entry 1 starts at offset 20+4+8 = 32
        let mut raw = std::fs::read(&path)?;
        if raw.len() > 35 {
            raw[32] ^= 0xFF; // flip first byte of entry 1's CRC
            std::fs::write(&path, &raw)?;
        }

        let entries = read_wal(&path)?;
        assert_eq!(entries.len(), 1, "Should read only entry 0; entry 1 is corrupt");

        std::fs::remove_dir_all(&dir).ok();
        Ok(())
    }

    #[test]
    fn wal_tolerates_empty_file() -> Result<()> {
        let dir  = tmp_dir("wal_empty");
        let path = dir.join("empty.wal");
        std::fs::write(&path, b"")?;
        let entries = read_wal(&path)?;
        assert_eq!(entries.len(), 0);
        std::fs::remove_dir_all(&dir).ok();
        Ok(())
    }

    #[test]
    fn wal_tolerates_missing_file() -> Result<()> {
        let path = PathBuf::from("/tmp/cmhm_nonexistent_77421.wal");
        let entries = read_wal(&path)?;
        assert_eq!(entries.len(), 0);
        Ok(())
    }

    // --- Snapshot generations ---------------------------------------------------

    #[test]
    fn snapshot_round_trip() -> Result<()> {
        let dir  = tmp_dir("snap_rt");
        let path = dir.join("state.bin");

        write_snapshot(&42u32, &path)?;
        let v: u32 = read_snapshot(&path).expect("round-trip must succeed");
        assert_eq!(v, 42);

        std::fs::remove_dir_all(&dir).ok();
        Ok(())
    }

    #[test]
    fn snapshot_keeps_generations() -> Result<()> {
        let dir  = tmp_dir("snap_gen");
        let path = dir.join("state.bin");

        write_snapshot(&1u32, &path)?;
        write_snapshot(&2u32, &path)?;
        write_snapshot(&3u32, &path)?;

        // Current must be 3
        let v: u32 = read_snapshot(&path).expect("current snapshot must load");
        assert_eq!(v, 3, "Current snapshot must be latest");

        // .1 must exist and be 2
        let gen1: u32 = read_snapshot(&generation_path(&path, 1)).expect(".1 must load");
        assert_eq!(gen1, 2, "Generation .1 must be the previous save");

        // .2 must exist and be 1
        let gen2: u32 = read_snapshot(&generation_path(&path, 2)).expect(".2 must load");
        assert_eq!(gen2, 1, "Generation .2 must be oldest");

        std::fs::remove_dir_all(&dir).ok();
        Ok(())
    }

    #[test]
    fn snapshot_fallback_to_generation() {
        let dir  = tmp_dir("snap_fallback");
        let path = dir.join("state.bin");

        write_snapshot(&99u32, &path).unwrap();
        write_snapshot(&100u32, &path).unwrap();

        // Corrupt the primary: write a valid-version envelope but a 3-byte payload
        // (bincode needs ≥4 bytes for u32 → UnexpectedEof → Corrupt → falls back to .1).
        let corrupt_envelope = SnapshotEnvelope {
            version: SNAPSHOT_VERSION,
            payload: vec![0xAB, 0xCD, 0xEF], // 3 bytes — UnexpectedEof decoding u32
        };
        let bytes = bincode::serialize(&corrupt_envelope).unwrap();
        std::fs::write(&path, &bytes).unwrap();

        // read_snapshot must fall back to .1 = 99
        let v: u32 = read_snapshot(&path).expect("should fall back to .1 on Corrupt primary");
        assert_eq!(v, 99, "Should fall back to generation .1 when primary payload is corrupt");

        std::fs::remove_dir_all(&dir).ok();
    }


    // ── New: WAL torn-write recovery ──────────────────────────────────────────

    /// A truncated final record (torn write) must be silently ignored.
    /// Only the preceding complete records should be returned.
    #[test]
    fn wal_torn_write_truncated_payload() -> Result<()> {
        let dir  = tmp_dir("wal_torn");
        let path = dir.join("wal.dat");

        // Write one clean record.
        let mut w = WalWriter::open(&path)?;
        w.append(b"hello-world")?;
        drop(w);

        // Append partial bytes: a length field (say 1024) with no payload.
        // This simulates a torn write where the process died after writing len
        // but before writing the payload.
        let partial_len: u32 = 1024;
        let mut file = std::fs::OpenOptions::new().append(true).open(&path)?;
        file.write_all(&partial_len.to_le_bytes())?;
        // (no payload, no CRC)
        drop(file);

        let entries = read_wal(&path)?;
        assert_eq!(entries.len(), 1, "Torn tail must be silently truncated; only complete records returned");
        assert_eq!(&entries[0].payload, b"hello-world");

        std::fs::remove_dir_all(&dir).ok();
        Ok(())
    }

    /// A CRC-mismatched record stops replay; preceding records survive.
    #[test]
    fn wal_corrupt_payload_crc_flip_stops_replay() -> Result<()> {
        let dir  = tmp_dir("wal_crc_flip");
        let path = dir.join("wal.dat");

        // Write two records.
        let mut w = WalWriter::open(&path)?;
        w.append(b"good-record")?;
        w.append(b"will-be-corrupted")?;
        drop(w);

        // Read the raw file, find the second record's payload, flip a bit.
        let mut bytes = std::fs::read(&path)?;
        // File layout: [MAGIC:4][len:4][payload:N][crc:4][len:4][payload:M][crc:4]
        // First record: len=11 ("good-record"), payload starts at offset 8.
        // Second record starts at: 4 + 4 + 11 + 4 = 23.
        // Second record payload starts at: 23 + 4 = 27.
        let second_payload_start = 4 + 4 + b"good-record".len() + 4 + 4;
        if second_payload_start < bytes.len() {
            bytes[second_payload_start] ^= 0xFF; // flip all bits in first byte of second payload
        }
        std::fs::write(&path, &bytes)?;

        let entries = read_wal(&path)?;
        assert_eq!(entries.len(), 1,
            "CRC mismatch on second record must stop replay; first record must survive");
        assert_eq!(&entries[0].payload, b"good-record");

        std::fs::remove_dir_all(&dir).ok();
        Ok(())
    }

    // ── New: Snapshot version classification ──────────────────────────────────

    /// A snapshot written with the wrong version tag must produce VersionMismatch,
    /// NOT Corrupt. The caller uses this to trigger WAL rebuild without alerting monitoring.
    #[test]
    fn snapshot_version_mismatch_is_distinct_from_corrupt() {
        let dir  = tmp_dir("snap_ver_mismatch");
        let path = dir.join("state.bin");

        // Write an envelope with a wrong version number directly.
        let wrong_envelope = SnapshotEnvelope {
            version: SNAPSHOT_VERSION + 1, // future version
            payload: bincode::serialize(&999u32).unwrap(),
        };
        let bytes = bincode::serialize(&wrong_envelope).unwrap();
        std::fs::write(&path, &bytes).unwrap();

        let result: std::result::Result<u32, SnapshotLoadError> = read_snapshot(&path);
        match result {
            Err(SnapshotLoadError::VersionMismatch { found, expected, .. }) => {
                assert_eq!(found, SNAPSHOT_VERSION + 1);
                assert_eq!(expected, SNAPSHOT_VERSION);
            }
            Err(SnapshotLoadError::Corrupt(_)) =>
                panic!("version mismatch must NOT be classified as Corrupt"),
            Ok(_) =>
                panic!("must not succeed with wrong version"),
            Err(SnapshotLoadError::NotFound) =>
                panic!("file exists — must not be NotFound"),
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Raw bytes (no envelope) — e.g., a snapshot from before versioning was added —
    /// must produce VersionMismatch (version=1 implied), NOT Corrupt.
    #[test]
    fn snapshot_pre_versioned_is_version_mismatch_not_corrupt() {
        let dir  = tmp_dir("snap_pre_ver");
        let path = dir.join("state.bin");

        // Write raw u32 bytes with no envelope — mimics a v1 (pre-envelope) snapshot.
        let raw_bytes = bincode::serialize(&42u32).unwrap();
        std::fs::write(&path, &raw_bytes).unwrap();

        let result: std::result::Result<u32, SnapshotLoadError> = read_snapshot(&path);
        match result {
            Err(SnapshotLoadError::VersionMismatch { found, expected, .. }) => {
                assert_eq!(found, 1, "pre-versioned implies version=1");
                assert_eq!(expected, SNAPSHOT_VERSION);
            }

            Err(SnapshotLoadError::Corrupt(_)) =>
                panic!("pre-versioned snapshot must NOT be classified as Corrupt"),
            Ok(_) =>
                panic!("must not deserialize successfully from wrong format"),
            Err(SnapshotLoadError::NotFound) =>
                panic!("file exists — must not be NotFound"),
        }

        std::fs::remove_dir_all(&dir).ok();
    }
}
