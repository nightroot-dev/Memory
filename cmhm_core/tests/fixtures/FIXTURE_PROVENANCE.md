# Immutable Fixture Provenance & Migration Specification

## 1. Fixture Inventory & Cryptographic Checksums

The binary fixtures stored in `cmhm_core/tests/fixtures/` are checked-in, immutable ground-truth artifacts used to guarantee schema migration and WAL replay backward compatibility.

| Fixture File | Size | SHA-256 Checksum | Format & Purpose |
|---|---|---|---|
| `v2_snapshot.bin` | 792,317 bytes | `121DDF7A3ED658DFED4F9FFE4C152D801653CC7DB12A914494EF2860EB4D1C9F` | `SnapshotEnvelope` (version = 2) containing serialized `CoreStateV2`. Tests in-memory migration to v3 without quarantine or WAL replay. |
| `legacy_v0_wal.dat` | 1,598 bytes | `BBAF6EC70F724CE439C33859DD3283255A287C655A2AFD9D79DF6B568ED28C2C` | `CWAL` format with `0x00` record types (v0/v1/v2 format). Tests fallback state reconstruction when snapshot is missing or corrupted. |

---

## 2. Legacy Origin & Serializer Specification

### Historical Source Commit
- **Commit**: `870062e` (`test: benchmark v3 -- operational mixed-corpus retrieval (pre-declared gates)`)
- **Pre-Phase 2 Schema**: `SNAPSHOT_VERSION = 2`

### Legacy Struct Definition (`CoreStateV2` & `NodeV2` in Commit `870062e`)
```rust
#[derive(Serialize, Deserialize)]
pub struct NodeV2 {
    pub embedding: Vec<f32>,
    pub model_id: String,
    pub salience: f32,
    pub timestamp: u64,
    pub emotional_tag: f32,
    pub node_type: NodeType,
    pub n_retrievals: u32,
    pub last_retrieved: u64,
    pub half_life_days: f32,
    pub max_prediction_error: f32,
    pub bandit_alpha: f32,
    pub bandit_beta: f32,
    // Note: authority, provenance, superseded_by, assertion_key, polarity DID NOT EXIST in v2
}

#[derive(Serialize, Deserialize)]
pub struct NodeStoreV2 {
    pub nodes: HashMap<u64, NodeV2>,
    pub next_id: u64,
    pub global_ema: Vec<f32>,
    pub embed_dim: usize,
}

#[derive(Serialize, Deserialize)]
pub struct SnapshotEnvelopeV2 {
    pub version: u32,       // = 2
    pub payload: Vec<u8>,   // bincode::serialize(&CoreStateV2)
}
```

### Legacy WAL Record Format (Type `0x00`)
```
[CWAL magic: 4B][len: u32 LE][0x00: u8]
[state_emb: 128 f32 LE][action_emb: 128 f32 LE][outcome_emb: 128 f32 LE]
[prediction_error: f32 LE][emotional_tag: f32 LE]
[model_id_len: u64 LE][model_id_utf8]
[crc32: u32 LE]
```

---

## 3. Relationship: `SNAPSHOT_VERSION` vs `WAL Record Types`

1. **`SNAPSHOT_VERSION` (Currently `3`)**:
   - Encapsulated in `SnapshotEnvelope { version: u32, payload: Vec<u8> }`.
   - `found == 3`: Deserialized directly as `CoreState`.
   - `found == 2`: Handled by `migration::migrate_v2_to_v3(&payload)` in memory. Converts `NodeV2` to `Node` by preserving all fields and populating neutral defaults:
     - `authority = 0.50` (neutral trust)
     - `provenance = ""` (unpromoted / unknown)
     - `superseded_by = None`
     - `assertion_key = None`
     - `polarity = None`
   - `found < 2` or corrupt: Triggers `SnapshotLoadError::Corrupt` / quarantine fallback with WAL replay if `allow_incompatible_snapshot_reset = true`.

2. **WAL Record Type Schema**:
   - `0x00` (`WAL_TYPE_LEARN`): Legacy entry without authority/provenance. Replayed with neutral defaults (`authority = 0.50`, `provenance = ""`).
   - `0x01` (`WAL_TYPE_FEEDBACK`): Feedback update.
   - `0x02` (`WAL_TYPE_LEARN_V2`): Modern entry with explicit `authority`, `provenance`, `supersedes`, `assertion_key`, `polarity`.

---

## 4. Preservation & Fail-Safe Ranking Invariants

1. **State Preservation Invariant**:
   - Context UUID: preserved.
   - Node IDs (e.g. `101`): preserved without renumbering.
   - Node Types (`NodeType::State`, `Action`, `Outcome`): preserved.
   - Embeddings (`128-dim f32`): exact bit-level preservation.
   - Bandit parameters (`alpha`, `beta`), salience, and timestamps: preserved.

2. **Fail-Safe Ranking Invariant**:
   - **Against High Authority**: When a migrated node (`authority = 0.50`) competes at equal cosine distance against a newly learned authoritative node (`authority = 0.85`, `"corrected"`), the authoritative node wins.
   - **Against Refuted / Low Authority**: When a migrated node (`authority = 0.50`) competes at equal cosine distance against a refuted node (`authority = 0.10`, `"contradicted"`), the neutral migrated node safely wins.
   - **Supersession**: When a migrated node is superseded (`supersedes = Some(101)`), `node.superseded_by` is set and the legacy node is completely excluded from subsequent seed selection.
