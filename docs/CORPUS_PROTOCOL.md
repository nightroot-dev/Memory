# Session L Corpus Protocol v1.0 (docs/CORPUS_PROTOCOL.md)

Measurement instrument for all future recall work. Every lever (reranker,
embedder swap, fusion change) is judged on this corpus — so the corpus rules
are stricter than any bench that uses them.

## 1. Corpus shape

- 200 pairs total: 100 lexical + 100 paraphrase.
- Each pair: `(situation, action, outcome, query, kind, difficulty)`.
- `action`/`outcome` are distinct per pair (chain-accuracy needs them).
- Stored at `tools/corpus_l1.json`. Harness: `cmhm_core/tests/ops_bench_v2.rs`.

## 2. Lexical pair rules (tests BM25 + identifier recall)

- Each situation carries ONE unique rare token (`l1rare000`–`l1rare099`):
  alphanumeric, no separators, never reused anywhere in the corpus.
- Situation body uses content words unique to that pair where feasible;
  NO shared boilerplate templates (the Session B flaw: identical framing
  across all 40 made BM25 unable to discriminate).
- Query shares ONLY the rare token with its situation (plus stopwords).
- Difficulty: `easy` if the rare token is the only match; queries never
  contain a second pair's rare token (checked by script).

## 3. Paraphrase pair rules (tests semantic recall)

- Query and situation share ZERO content tokens. Content token = lowercase
  alphanumeric token, len >= 3, not in the stop list. Checked by script;
  violations are rewritten, not waived.
- Query must be answerable by a human reader: an annotator who reads the
  query must pick the right situation out of 5 distractors. If not, the
  pair is dropped or rewritten (no embedder-filtering — the v3 circularity
  flaw: pairs must NOT be selected through any embedder).
- Difficulty tags (author judgment, fixed at write time):
  - `easy`: same event, different words, clear anchor nouns differ but
    scenario is unmistakable (e.g. "checkout randomly fails" ←
    "payment processor returned intermittent 502 errors").
  - `medium`: requires domain knowledge to connect (e.g. "standby never
    took over traffic" ← "database failover stalled mid-promotion").
  - `hard`: vague symptom with many plausible causes (e.g. "results
    arrive late" ← "search index rebuilt slower than expected").
- Target distribution: ~30 easy / ~40 medium / ~30 hard. Hard pairs are
  EXPECTED to fail on small embedders — they calibrate the ceiling, and a
  lever that only wins easy pairs is documented as such.

## 4. Metrics (harness reports all four; no single number)

- `seed P@1`: retrieved seed node == target state node.
- `pool recall`: target state node in `post_filter_candidates`.
- `chain accuracy`: returned causal chain reaches the target outcome node.
- `MRR` over pool ranks.
- Plus per-difficulty breakdown for paraphrase (easy/medium/hard).

## 5. Integrity rules

- No pair selected, filtered, or tuned through any embedder. Ever.
- Corpus edits require re-running the checker script (`tools/check_corpus_l1.py`).
- Baseline recorded per embedder on first green run; later runs compare
  against it. Bench deltas under ~7pts on n=200 are noise until a
  significance note says otherwise (200 pairs ≈ ±3.5pts per 7-pair flip).
- The 40-pair honest corpora stay untouched for history; L1 supersedes them.
