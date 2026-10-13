# T4 NOTES.md (notes-file arm only)

# WAL replay idempotency (learned from audit)
- WAL replay is NOT idempotent: every restart re-applies already-snapshotted
  learns, duplicating nodes (e.g. 46→77 across restart).
- Root cause class is lifecycle, not dedup: the birth gate behaves correctly
  on live traffic; the bug is that settled WAL entries replay at all
  (shutdown compaction + crash watermark territory).
- Do NOT "fix" by tightening the birth/dedup gate — that moves recall and
  breaks pinned thresholds.
