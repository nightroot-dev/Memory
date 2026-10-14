# B11 — Registry lost-update orphans context data (unreachable, undeletable)

**Status:** reproduced (controlled, 4 steps). Ledger-ready for reviewer
transplant into `docs/master_bug_list.md` — this file is builder-owned;
the reviewer ledger was NOT touched, per the standing rule.
**Discovered:** 2026-10-14, while verifying the B10 cleanup claim (owner
asked for disk/registry state before repeating cleanup claims — that check
is what surfaced it).
**Code under test:** `55f107f` (last code commit; serving binary SHA256
8a5ea128…db4f). Repro artifact: `tools/eval_b8/repro_registry_lost_update.py`
(runs against any build of that exe; no repo state touched — temp dir only).

## Class / severity

- **Class:** crash-safety / concurrent-writers lost update (NOT corruption).
- **Severity proposal: HIGH.** Answers the owner's severity question: this
  happens during **normal writes**, not only during kills — and multiple
  concurrent server instances are this client's NORMAL mode (one MCP
  instance per agent session; 5 observed concurrently during B10, all
  sharing one data dir after the client reinstall began spawning servers
  with workspace CWD).

## Mechanism (code paths)

- `cmhm_core/src/context/registry.rs`:
  - `load()` (L37-75): reads `registry.toml` ONCE at startup into an
    in-memory `HashMap`. No file lock (fs2 is a dependency but is used for
    context-dir scope locks, not the registry).
  - `save()` (L79-90): rewrites the ENTIRE file from the in-memory map.
    Atomic per-write (tmp + rename) — atomicity is irrelevant here; the
    loss is between well-formed writes.
  - `create()` (L97-106) and `transition()` (L122-159) call `save()`.
- `cmhm_core/src/context/runtime.rs`: transitions fire during ordinary
  operation — ensure_loaded → Active (L814/824), eviction → Sleeping
  (L809), sleep sweeper (L665/734), recovery paths (L737/743/785/790) —
  so every long-lived instance periodically rewrites the file from its
  load-time snapshot plus its own deltas.
- With ≥2 instances on one data dir: instance A loads at time T0; instance
  B creates a context at T1 (persisted); instance A saves anything at T2
  → B's entry is erased. A cannot see it (map is stale), the API cannot
  delete it (E_NOT_FOUND), the folder stays on disk forever (no rescan,
  no GC, no reconciliation).

## Controlled reproduction (4 steps, all in temp dir)

Using `repro_registry_lost_update.py` (P1/P2/P3 = separate server
processes, same `data/`):

1. P1 spawns on empty dir, creates `ctx-early` → registry
   `{ctx-early, default(auto)}`.
2. P2 spawns (loads that registry), creates `ctx-late`, learns into it
   (folder `data/contexts/<uuid>/` materializes) → registry
   `{ctx-early, ctx-late, default}`; folder present.
3. **P1 — whose in-memory map predates `ctx-late` — creates `ctx-clobber`
   → its save rewrites the file: registry `{ctx-clobber, ctx-early,
   default}`. `ctx-late` GONE from the registry while its folder remains.**
4. Kill both; fresh P3: `context_list` cannot see `ctx-late`;
   `context_delete(uuid)` → `E_NOT_FOUND` — with the folder still on disk.
   Restart does NOT fix it: nothing rescans disk or reconciles.

Production incident (B10, same mechanism): after the client reinstall,
all sessions' servers ran with CWD = workspace root, so my session's
instance plus 7 arm instances shared `C:\Games\Projects\Memory\data`.
The 7 `b10-*` contexts (created + used by the arms) ended up as folders
with no registry entries; `context_list` showed only 3 stale contexts;
cleanup required direct disk removal. Full sequence recorded in
`results.md` (B10 cleanup correction, commit 73dab70).

## State at the time (production)

- `data/contexts/`: 7 b10-* UUID folders + probes (WALs/snapshots present).
- `data/registry.toml`: only 3 stale entries (default/VideoAI/CMHM,
  created 2026-07, 0 learns); mtime 21:32:58 = the last stale-map save
  (my session's instance had loaded pre-seeding; its later probe
  create/delete churn rewrote the file), consistent with the mechanism —
  exact instance attribution plausible but not separately proven.

## Answers to the owner's specific questions

- **Normal writes, not just kills?** YES — repro step 3 needs no kill;
  any create/transition/save from a stale-map instance clobbers. Kills
  only make it worse (unflushed state, as in the B10 cleanup incident).
- **Same path as F3 (registry corrupt → silent reset)?** NO — second gap.
  F3 is parse-corruption on load (quarantine + fresh start,
  `registry.rs` L41-64, test L390-411). Here the file is always
  well-formed; the loss is a multi-writer lost update. F3's fix does not
  and cannot cover this.
- **Do orphans come back after a restart?** They never leave: no disk
  rescan, no reconciliation, no GC. The folder persists; the API stays
  blind; delete stays E_NOT_FOUND.

## Fix directions (NOT implemented — reproduction before any fix, per owner)

1. Lock or single-writer: take an fs2 file lock around load+save, or
   route all registry writes through one owner process.
2. Read-merge-write: re-load the file immediately before every save and
   merge (last-writer-wins per entry, union of entries) instead of
   writing the whole stale map.
3. Reconciliation: on startup, scan `data/contexts/` and adopt folders
   missing from the registry (heals existing orphans).
4. Client-side: stop sharing one data dir across instances (per-session
   data dirs), or serialize MCP servers per workspace.
(1)+(3) together close both the write race and the existing-orphan class.

## Evidence

- Repro script + raw transcript: `tools/eval_b8/repro_registry_lost_update.py`.
- Production record: `results.md` B10 section, cleanup correction (73dab70).
- Code refs as cited above; no source modified for this entry.
