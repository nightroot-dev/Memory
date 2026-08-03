use std::fs::{File, OpenOptions};
use std::io::{Write, Read, Seek, SeekFrom, BufWriter, BufReader};
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use anyhow::{Result, anyhow};

// ─── WAL constants ────────────────────────────────────────────────────────────

/// Magic bytes written as the first 4 bytes of every new WAL file.
/// Lets `read_wal` detect misalignment and bail cleanly instead of reading garbage.
const WAL_MAGIC: &[u8; 4] = b"CWAL";

/// Maximum sane record size. Protects against corrupted length fields.
const WAL_MAX_RECORD_BYTES: usize = 64 * 1024 * 1024; // 64 MiB

/// Number of snapshot generations to keep.
/// Keep 3: current + 2 fallbacks. Never delete old until new is durable.
const SNAPSHOT_GENERATIONS: u32 = 3;

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
}

impl WalWriter {
    /// Open (or create) a WAL file.
    ///
    /// If the file is newly created (size == 0), writes the 4-byte magic header.
    /// If the file already exists, assumes magic was written on creation.
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

        Ok(Self { file: BufWriter::new(file), seq: 0 })
    }

    /// Append one serialized record to the WAL.
    ///
    /// Writes: `[len: u32 LE][payload][crc32: u32 LE]`
    /// Then flushes the buffer and calls `sync_data()` to guarantee durability.
    pub fn append(&mut self, cmd_bytes: &[u8]) -> Result<()> {
        self.seq += 1;
        let len = cmd_bytes.len() as u32;
        let crc = crc32(cmd_bytes);

        self.file.write_all(&len.to_le_bytes())?;
        self.file.write_all(cmd_bytes)?;
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
            if let Some(entry) = read_one_record(&mut reader, len)? {
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
        match read_one_record(&mut reader, len)? {
            Some(entry) => entries.push(entry),
            None        => break, // CRC mismatch or short read — stop here
        }
    }

    tracing::info!("WAL replay: {} valid entries", entries.len());
    Ok(entries)
}

/// Read exactly one record (payload + CRC) given the length field was already read.
/// Returns `None` on any read error or CRC mismatch — caller stops replay.
fn read_one_record<R: Read>(reader: &mut R, len: usize) -> Result<Option<WalEntry>> {
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

/// Write a bincode snapshot atomically, keeping `SNAPSHOT_GENERATIONS` old copies.
///
/// ## Write sequence (prevents partial-write corruption)
/// 1. Serialize to `path.tmp`
/// 2. `sync_all()` on the `.tmp` file  ← data + metadata durable
/// 3. Rotate generations: `.2 ← .1 ← current ← .tmp`
/// 4. `sync_all()` on parent directory ← rename durable (POSIX only)
///
/// On Windows, step 4 is a no-op with a trace-level warning. Windows NTFS
/// journals renames, so data-loss risk is low but not zero.
///
/// ## Generation rotation
/// On every save: current → `.1`, `.1` → `.2`, `.2` is deleted.
/// If the current save fails, the caller still has `.1` and `.2` as fallbacks.
pub fn write_snapshot<T: Serialize>(state: &T, path: &PathBuf) -> Result<()> {
    let bytes = bincode::serialize(state)?;
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

/// Deserialize snapshot from disk.
///
/// Falls back through generations (.1, .2) if the primary is missing or corrupt.
/// Never panics — returns `Err` if all generations fail.
pub fn read_snapshot<T: for<'de> Deserialize<'de>>(path: &PathBuf) -> Result<T> {
    // Try current, then .1, then .2
    let candidates = [
        path.clone(),
        generation_path(path, 1),
        generation_path(path, 2),
    ];

    let mut last_err = anyhow!("no snapshot file exists at {:?}", path);
    for candidate in &candidates {
        if !candidate.exists() { continue; }
        match std::fs::read(candidate) {
            Err(e) => { last_err = e.into(); continue; }
            Ok(bytes) => match bincode::deserialize::<T>(&bytes) {
                Err(e) => {
                    tracing::warn!("Snapshot {:?} corrupt ({e}), trying next generation", candidate);
                    last_err = e.into();
                }
                Ok(state) => {
                    tracing::info!("Snapshot loaded from {:?}", candidate);
                    return Ok(state);
                }
            }
        }
    }
    Err(last_err)
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
        let v: u32 = read_snapshot(&path)?;
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
        let v: u32 = read_snapshot(&path)?;
        assert_eq!(v, 3, "Current snapshot must be latest");

        // .1 must exist and be 2
        let gen1: u32 = read_snapshot(&generation_path(&path, 1))?;
        assert_eq!(gen1, 2, "Generation .1 must be the previous save");

        // .2 must exist and be 1
        let gen2: u32 = read_snapshot(&generation_path(&path, 2))?;
        assert_eq!(gen2, 1, "Generation .2 must be oldest");

        std::fs::remove_dir_all(&dir).ok();
        Ok(())
    }

    #[test]
    fn snapshot_fallback_to_generation() -> Result<()> {
        let dir  = tmp_dir("snap_fallback");
        let path = dir.join("state.bin");

        write_snapshot(&99u32, &path)?;
        write_snapshot(&100u32, &path)?;

        // Corrupt the primary by truncating to 2 bytes.
        // bincode::deserialize::<u32> needs exactly 4 bytes → will fail with IoError.
        // This proves read_snapshot falls back to generation .1 on deserialise failure.
        std::fs::write(&path, b"\xFF\xFF")?;

        // read_snapshot must fall back to .1 = 99
        let v: u32 = read_snapshot(&path)?;
        assert_eq!(v, 99, "Should fall back to generation .1 when primary is corrupt");

        std::fs::remove_dir_all(&dir).ok();
        Ok(())
    }
}
