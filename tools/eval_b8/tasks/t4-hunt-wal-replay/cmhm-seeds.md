# T4 CMHM seed entries (cmhm arm only — isolated `b8-t4` context)

1. state: "nodes duplicate across restart in persistence layer"
   action: "checked WAL replay path: snapshotted entries replay without a watermark"
   outcome: "root cause is lifecycle (compaction/watermark), not the dedup gate"
   prediction_error: 0.9
2. state: "tempted to tighten birth dedup for replay dups"
   action: "rejected: gate behaves correctly on live traffic, would move recall"
   outcome: "fix belongs at shutdown compaction + crash watermark"
   prediction_error: 0.6
