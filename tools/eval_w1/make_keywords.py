#!/usr/bin/env python3
"""W1 keyword fixture generator — mechanical derivation from corpus_l1.json ONLY.

Registered rules (tools/eval_b8/w1-write-time-keywords.md, committed BEFORE
this script runs in anger):

- kw_transcript (W1a): distinct content tokens of the item's QUERY minus the
  stopword list, order preserved, truncated at 10 tokens. Models the agent
  transcribing the caller's phrasing into the saved memory text.
- kw_blind (W1b): first 4 distinct content tokens of the SITUATION minus the
  stopword list. Store-side only; the BM25 index already holds these, so
  this leg is the mechanism control (expected ~flat on para-hard).
- Numerals are kept (discriminative: "500mbps", "006").
- Tokenizer: lowercase ASCII [A-Za-z0-9_]+ runs (splits on punctuation,
  including hyphens) — matches the coarse split used for the self-checks.

Self-checks abort generation (exit 1) if violated:
  1. n=200 items; both keyword strings non-empty for every item.
  2. para-hard: kw_blind ∩ query content tokens == ∅ (corpus-guaranteed).
  3. para-hard: kw_transcript ∩ base-text content tokens == ∅ (the bridge
     genuinely adds new vocabulary, not a restatement of store tokens).
  4. Prints per-slice disclosure counts into the pre-run block.

Output: tools/eval_w1/corpus_l1_kw.json — original item fields verbatim,
plus "kw_transcript" and "kw_blind". No manual edits after generation.
"""

import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent          # tools/eval_w1 -> tools
SRC = REPO / "corpus_l1.json"
OUT = REPO / "eval_w1" / "corpus_l1_kw.json"

# Pre-registered stopword list (function words + question words only).
STOPWORDS = {
    "a", "an", "the", "and", "or", "but", "if", "of", "in", "on", "at", "to",
    "for", "with", "from", "as", "is", "are", "was", "were", "be", "been",
    "being", "it", "its", "this", "that", "these", "those", "what", "which",
    "who", "whom", "whose", "why", "how", "when", "where", "do", "does",
    "did", "done", "can", "could", "should", "would", "will", "not", "no",
    "yes", "up", "down", "out", "over", "under", "by", "my", "your", "our",
    "their", "his", "her", "i", "we", "you", "they", "then", "so", "due",
    "via", "about", "into", "through", "during", "before", "after", "above",
    "below", "again", "further", "once", "here", "there", "all", "any",
    "both", "each", "few", "more", "most", "other", "some", "such", "only",
    "own", "same", "too", "very", "just", "than", "also", "caused", "cause",
    "causing", "got", "get",
}

TOKEN_RE = re.compile(r"[a-z0-9_]+")


def toks(text: str) -> list[str]:
    return TOKEN_RE.findall(text.lower())


def content_toks(text: str) -> list[str]:
    return [t for t in toks(text) if t not in STOPWORDS]


def distinct(seq: list[str], cap: int) -> list[str]:
    seen: set[str] = set()
    out: list[str] = []
    for t in seq:
        if t not in seen:
            seen.add(t)
            out.append(t)
            if cap and len(out) >= cap:
                break
    return out


def main() -> int:
    items = json.loads(SRC.read_text(encoding="utf-8"))
    if len(items) != 200:
        print(f"ABORT: source corpus has {len(items)} items, expected 200", file=sys.stderr)
        return 1

    slices = {"lexical": 0, "easy": 0, "medium": 0, "hard": 0}
    bridge_shared = {"hard": 0, "medium": 0, "easy": 0, "lexical": 0}
    blind_shared = {"hard": 0, "medium": 0, "easy": 0, "lexical": 0}

    for it in items:
        q = content_toks(it["query"])
        base = content_toks(
            it["situation"] + " " + it["action"] + " " + it["outcome"]
        )
        s = it["situation"]

        kw_t = distinct(q, 10)
        kw_b = distinct(content_toks(s), 4)

        if not kw_t or not kw_b:
            print(f"ABORT: empty keyword set for {it['id']}", file=sys.stderr)
            return 1

        q_set = set(q)
        base_set = set(base)
        kt_set = set(kw_t)
        kb_set = set(kw_b)

        d = it["difficulty"]
        k = "lexical" if it["kind"] == "lexical" else d
        if kt_set & q_set:
            bridge_shared[k] += 1
        if kb_set & q_set:
            blind_shared[k] += 1

        # Hard-slice guarantees the design depends on:
        if d == "hard":
            if kb_set & q_set:
                print(f"ABORT: hard {it['id']} kw_blind overlaps query tokens "
                      f"{sorted(kb_set & q_set)} — corpus invariant broken", file=sys.stderr)
                return 1
            if kt_set & base_set:
                print(f"ABORT: hard {it['id']} kw_transcript overlaps base "
                      f"tokens {sorted(kt_set & base_set)} — bridge must ADD "
                      f"vocabulary; revise design BEFORE any run", file=sys.stderr)
                return 1

        slices[k] += 1
        it["kw_transcript"] = " ".join(kw_t)
        it["kw_blind"] = " ".join(kw_b)

    print(f"n=200 fixture generated: {slices}")
    print("pre-run disclosure (items with query-token overlap):")
    print(f"  kw_transcript shares >=1 query token: {bridge_shared}")
    print(f"  kw_blind shares >=1 query token:      {blind_shared}")
    ex_hard = [it for it in items if it["difficulty"] == "hard"][:3]
    for it in ex_hard:
        print(f"  hard example {it['id']}: situation='{it['situation']}'")
        print(f"    query='{it['query']}'")
        print(f"    kw_transcript='{it['kw_transcript']}' kw_blind='{it['kw_blind']}'")
    OUT.write_text(json.dumps(items, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"written: {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
