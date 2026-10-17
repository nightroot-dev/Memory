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

- `data/contexts/e871b8ed-145e-4c18-bc3a-e8dc87fe5a00` holds a 26,726-byte
  un-compacted `wal.dat` (last write 2026-10-17 11:52:33) next to
  `snapshot.bin` (11:30:06): a crash-recovery tail left by the session that
  died while the context was active. Its next load goes through the same
  S-05 machinery (`ensure_loaded` → `engine::start`). The store-root
  `wal.dat` is 0 bytes (compaction-bounded).
- Serving binary unchanged: GNU-path `acde87b2…` (includes S-05; rebuilt
  after B11).

## Commits

- `fd91d6f` — test(s05): S05-4 repro (this record's test; its own commit).
- `69f603a` — historical S-05 fix (R1's actual fix commit; already shipped).
- This record + the results.md pointer = the docs commit following it.

No `docs/master_bug_list.md` / `task.md` edits (reviewer seat).
