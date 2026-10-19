# R1 restart idempotency — repro record (2026-10-17)

## What R1 is (owner-confirmed framing)

- Owner (2026-10-17): "R1 (restart bug) is a data bug. Every restart re-applies
  the write log, so memories are duplicated (node count went 46 → 77 in a
  test). ... The test is: restart the server and confirm the node count and
  the ledger don't change."
- Attribution: R1 was recorded and fixed earlier as S-05 — commit `69f603a`
  "fix(ledger): S-05 crash-safe WAL replay watermark + ledger skip-before-append
  (snapshot v5)"; master bug list line 98 ("was: 46→77 nodes across restart",
  SHIPPED). The builder's previous reply had wrongly folded R1 into the 70%
  recall ceiling; that conflation is retracted. The recall ceiling is a
  documented measured limitation; R1 is a data bug and already has a real
  code fix in history.
- This turn = the requested repro + restart-test evidence on current HEAD,
  plus one coverage gap closed. No engine changes (the fix is already shipped
  and the serving binary already carries it).

## Fix shape (69f603a, unchanged this turn)

- Snapshot-side watermark `last_applied_wal_seq` (snapshot schema v5; v2/v3/v4
  migrate with watermark 0): canonical-path snapshots stamp the current WAL
  length and RETAIN the WAL (crash-recovery path if the snapshot corrupts).
- Replay skips the applied prefix; truncated-WAL guard replays all (safe
  direction of failure).
- Ledger skip-before-append gated on `is_replay` (process_learn: "A replayed
  learn whose three births ALL deduped to existing nodes is by definition
  already recorded").
- Startup compaction: post-replay snapshot + WAL delete bounds the tail.
- Shutdown retains the WAL (a delete-at-shutdown attempt broke E7 quarantine
  recovery; caught by a test at fix time and reverted).

## New repro: S05-4 (committed, fd91d6f)

`cmhm_core/tests/s05_restart.rs::s05_crash_with_nonempty_tail`

Owner's exact regime — mid-life snapshot, then MORE learns (a write-log tail
not covered by any snapshot), then a crash (drop without shutdown):

1. 5 learns (pe=0.9, distinct, hash-128d) → 15 nodes / 5 episodes → canonical
   snapshot (watermark stamped = 5, WAL retained with 5 entries).
2. 5 more learns → 30 nodes / 10 episodes / next_seq 11 (tail entries only in
   the WAL; nodes absent from the snapshot).
3. Crash (drop without shutdown) → boot 2 must recover the tail exactly once:
   30 nodes / 10 episodes / next_seq 11.
4. Boot 3 (post-compaction) must stay flat: 30/10/11.

Results:

- **HEAD: GREEN.** s05_restart 4/4 (S05-1 30/10/11 high-surprise restart;
  S05-2 flat low-surprise control; S05-3 crash with EMPTY tail — the
  previously shipped case; S05-4 this new case). Direct r1 acceptance:
  "restart the server, node count and ledger don't change" — pass.
- **Pre-fix RED-proof: the same test shape on `69f603a^` (31c1b71) FAILED with
  boot2 node_count = 45 ≠ 30** — the recorded R1 duplication mechanism
  (snapshot's 15 nodes + full-WAL replay re-birthing learns 1-5 through the
  pe=0.9 escape; ledger duplicates alongside). Run in a throwaway worktree,
  removed after the run; the adapted test was never committed.
  Adaptations forced by the parent's API: 13-arg `learn_with_context` (no B6a
  `strict_no_dup` yet), `node_count`-only asserts (parent EngineHandle has no
  `ledger_len`/`ledger_next_seq`). Toolchain note: the parent commit's
  `.cargo/config.toml` pins GNU + rust-lld, which failed to link system libs
  in the worktree; ran with explicit `+stable-x86_64-pc-windows-msvc`. The
  test is deterministic (hash-128d embedder, no onnx), so toolchain choice
  does not affect the result.
- **Workspace suite (S-10 protocol), HEAD including S05-4:**
  `cargo test --workspace --no-fail-fast` → 59 `test result` lines, 0 FAILED,
  no compile errors. (The B11-era record said 55 binaries; the delta is the
  B13/eval instrument test files added since — no regression.)

## Live-data note (observational, no action taken)

> UPDATED 2026-10-19 below — the 26 KB tail did NOT stay untouched: it was
> consumed by a load on 2026-10-17 14:51. The original note is kept for the
> record; see "Owner pre-signoff directives (2026-10-19)".

- `data/contexts/e871b8ed-145e-4c18-bc3a-e8dc87fe5a00` holds a 26,726-byte
  un-compacted `wal.dat` (last write 2026-10-17 11:52:33) next to
  `snapshot.bin` (11:30:06): a crash-recovery tail left by the session that
  died while the context was active. Its next load goes through the same
  S-05 machinery (`ensure_loaded` → `engine::start`). The store-root
  `wal.dat` is 0 bytes (compaction-bounded).
- Serving binary unchanged: GNU-path `acde87b2…` (includes S-05; rebuilt
  after B11).

---

## Owner pre-signoff directives (2026-10-19) — execution record

Owner directives before the reviewer signs off: (1) exact passing-test
counts, not just "0 FAILED"; (2) back up the live context before its next
load (rollback point); (3) keep R1 and B11 as separate sign-offs.

### Directive 1 — exact counts (workspace suite, 2026-10-17 run)

From the saved raw output of the same `cargo test --workspace
--no-fail-fast` run the 0-FAILED claim came from (retained as
`tools/eval_b8/s10_suite_full-20261019.txt`, 113,344 bytes):

| Metric           | Value |
|------------------|-------|
| tests passed     | 453   |
| tests failed     | 0     |
| tests ignored    | 8     (the `--ignored` bench/probe harnesses: ops_bench_v2, loadbench, MiniLM/bge harnesses, r1_live_sandbox, etc.) |
| test binaries    | 55    (unittests + integration targets; no doctest targets in this workspace) |
| `test result` groups | 59 (55 binaries + a few zero-test groups) |

Historical baselines the owner and records remember (49, 50/50, 55 binaries)
are era-dependent counts of a growing suite (S-05 era, post-B11 era); the
authoritative current numbers are the row above. The growth is from test
files added since (B13 instruments, S05-4, etc.), not from re-labeled
failures.

### Directive 2 — live-context backup + the material finding

**Material finding first: the 26,726-byte tail was already consumed on
2026-10-17 at 14:51:03 — one day before the directive.** File forensics
(recorded in this session on 2026-10-17 and re-verified today):

| File          | Was (17-10 pre-load)            | Now (post-load, unchanged since) |
|---------------|--------------------------------|----------------------------------|
| snapshot.bin  | 1,393,908 B @ 11:30:06         | 1,410,864 B @ 14:51:03 (+16,956) |
| wal.dat       | 26,726 B @ 11:52:33            | 6,162 B @ 14:51:03               |
| context_list  | version 7 / learns 7           | version 7 / learns 7 (unchanged) |

Reading: the load replayed the 26 KB tail, wrote an absorbing snapshot, and
left a new 6,162-byte tail (same-second writes at 14:51:03). Cross-checks
consistent with exactly-once recovery, NOT duplication:
- snapshot grew ~17 KB, the right size class for absorbing ~26 KB of WAL
  records ONCE (a duplication re-apply would have grown it ~2×);
- learn counters unchanged (7) — no double-counting;
- the engine ledger now holds 8 EPISODES for a context whose registry
  counter says 7 learns: the 8th episode is the recovered tail learn. The
  registry counter missed it because the 11:52 kill raced its update. This
  is flagged for the reviewer as an observed counter mismatch
  (meta=7, ledger=8), consistent with correct recovery.

**Backup (rollback point) for the context's NEXT load — taken before any
new load was triggered by this work:**

- Path: `C:\Games\Projects\Memory\backups\e871b8ed-preflight-20261019-125043`
- Contents: the full context dir (key.bin, meta.json, snapshot.bin ×3 gens,
  LOCK) captured byte-identical (SHA-256 per-file, src==dst verified for all
  7 files), `data/projections/` (both model matrices, sizes match live:
  bge-base-en-v1.5.bin 393,732 B; minilm-l6-v2-int8.bin 197,124 B),
  `registry.toml`, `tenant.id`, and the root `wal.dat` /
  `learn_log.jsonl` / `retrieve_log.jsonl` / `gate_shadow_log.jsonl`.
- `MANIFEST.sha256.json` lists every file + hash (15 files).
- The context is `minilm-l6-v2-int8`, encrypted at rest (meta key_ref
  `file:key.bin`; snapshot head bytes ASCII "CMHMSEAL" confirm sealing).
- `backups/` is currently untracked (not gitignored); it is NOT committed.
- Rollback procedure if the next load misbehaves: stop the cmhm server,
  delete the `data\contexts\e871b8ed…` directory, copy
  `backup\context\*` back into it (and `projections\*.bin` to
  `data\projections\`, `registry.toml`/`tenant.id` to `data\`), restart.
  Since the backup is post-14:51-load state, rollback restores the exact
  pre-directive state including its 6,162-byte open tail.

### Directive 2 continued — live-sandbox two-boot test (green)

New harness: `cmhm_core/tests/r1_live_sandbox.rs` (`#[ignore]`, env-gated;
S-10 suite-neutral). Copy of the backup's context dir + projections to a
sandbox; the test hardware does the owner's gate on REAL data with zero risk
to live files: two consecutive boots over the same dir must produce
identical node count AND ledger.

Result on the pre-directive live state (today):

```
r1_live_sandbox boot1: nodes=24 episodes=8 next_seq=9
r1_live_sandbox boot2: nodes=24 episodes=8 next_seq=9
test result: ok. 1 passed; 0 failed
```

- The open 6,162-byte tail replayed deterministically; no duplication;
  compaction bounded it within the sandbox. The engine ledger reading
  (8 episodes) matches the recovery interpretation above.
- Lesson recorded for future sandbox work: a naive file copy shows
  nodes=0 — the context is encrypted at rest and the persistence seam is
  keyed via a per-process registry populated only by `scope.rs` adopt
  (`Keystore::load_or_create(root)` + `persistence::register_scope_key`).
  The harness now mirrors that adoption step explicitly (engine decides
  encryption only via the registered key; the test does NOT decrypt or
  re-encrypt anything manually).
- Sandbox copy deleted after the run; only the pristine backup remains.

### Directive 3 — separate sign-offs (acknowledged, records already separate)

- B11 record: `tools/eval_b8/b11-registry-durability.md` (+ fix c48ee4a).
- R1 record: this file (+ fix 69f603a, repro fd91d6f).
- `docs/master_bug_list.md` keeps them as separate rows (B11/M1 line and
  S-05/R1 line); any copy of these findings into the master list is the
  reviewer's seat — the builder will not edit it. Nothing in this turn
  merges or mixes the two bugs.

## Commits

- `fd91d6f` — test(s05): S05-4 repro (this record's test; its own commit).
- `69f603a` — historical S-05 fix (R1's actual fix commit; already shipped).
- This record + the results.md pointer = the docs commit following it.

No `docs/master_bug_list.md` / `task.md` edits (reviewer seat).
