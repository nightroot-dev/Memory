# T5 NOTES.md (notes-file arm only)

# BM25/retrieval scoring pitfalls (learned from audit)
- `Bm25Index::insert` calls `remove()` (which already decrements
  `total_length`/`num_docs`) and then subtracts the old length AGAIN —
  corrupts avgdl on every re-index. Check length accounting first.
- RRF fusion scale mismatch can let the authority term override relevance in
  retrieval scoring — when a trusted-but-irrelevant node wins, compare the
  score-term magnitudes before anything else.
