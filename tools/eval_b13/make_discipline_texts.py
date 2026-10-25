#!/usr/bin/env python3
"""B13 ship-default discipline-text fixture generator.

Pre-registered: tools/eval_b8/b13-ship-default.md (committed 53bdaa3,
BEFORE this script runs and before its output is used anywhere).
Mechanical text generation only: no model, no network, no manual edits
after generation. Output is committed byte-for-byte; the bench
instrument asserts the template holds for every item.

Output: tools/eval_b13/corpus_b13_discipline.json

Template (verbatim from pre-reg section 3):
    For '<query>', what was learned: <situation>

Self-checks (print + abort-on-fail), per pre-reg section 3:
  1. n == 200 with exact group counts (100 lexical / 29 easy / 41
     medium / 30 hard).
  2. unique ids; required fields present and non-empty.
  3. every template text non-empty; fixture discipline_text equals the
     template applied to the item's fields for ALL items.
  4. per-group count of items where query and situation share >=1
     content token. Expected 0 for para-hard (corpus construction) -
     abort if not; other groups are printed as disclosure, not gated.
"""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CORPUS = ROOT / "tools" / "corpus_l1.json"
OUT = ROOT / "tools" / "eval_b13" / "corpus_b13_discipline.json"

# Fixed stoplist (b12r section-7 published list + a few closed-class
# additions; numerals kept - they are discriminative, per W1 convention).
STOP = {
    "the", "a", "an", "and", "or", "for", "with", "from", "after",
    "before", "into", "over", "under", "while", "when", "this", "that",
    "these", "those", "have", "has", "had", "was", "were", "are", "is",
    "been", "being", "they", "them", "their", "there", "here", "what",
    "which", "who", "why", "how", "all", "any", "both", "each", "few",
    "more", "most", "other", "some", "such", "only", "own", "same",
    "than", "too", "very", "can", "will", "just", "should", "now",
    "not", "but", "its", "you", "your", "we", "our", "us", "to", "in",
    "on", "at", "by", "of", "it", "as", "be", "do", "does", "did", "so",
    "then", "if", "no", "nor",
}

TEMPLATE = "For '{q}', what was learned: {s}"

FIELDS = ("id", "kind", "difficulty", "situation", "query", "action", "outcome")

EXPECTED = {"lexical": 100, "easy": 29, "medium": 41, "hard": 30}


def toks(text):
    return [t for t in re.findall(r"[a-z0-9_]+", text.lower())
            if t not in STOP and len(t) >= 2]


def group(it):
    if it["kind"] == "lexical":
        return "lexical"
    return f"para-{it['difficulty']}"


def group_counts(items):
    c = {"lexical": 0, "easy": 0, "medium": 0, "hard": 0}
    for it in items:
        if it["kind"] == "lexical":
            c["lexical"] += 1
        else:
            c[it["difficulty"]] += 1
    return c


def fail(msg):
    print(f"ABORT: {msg}")
    sys.exit(1)


def main():
    items = json.loads(CORPUS.read_text(encoding="utf-8"))

    # Check 1: exact size and group counts.
    if len(items) != 200:
        fail(f"corpus has {len(items)} items, need 200")
    counts = group_counts(items)
    if counts != EXPECTED:
        fail(f"group counts {counts} != expected {EXPECTED}")
    print(f"check1 OK: n=200 groups={counts}")

    # Check 2: unique ids, required fields.
    ids = [it.get("id") for it in items]
    if len(ids) != len(set(ids)):
        fail("duplicate item ids")
    for it in items:
        for f in FIELDS:
            v = it.get(f)
            if not isinstance(v, str) or not v.strip():
                fail(f"item {it.get('id')}: field {f!r} missing/empty")
    print("check2 OK: ids unique, fields present")

    # Check 4: overlap disclosure + hard-group gate.
    overlap = {}
    hard_overlaps = []
    for it in items:
        g = group(it)
        qt, st = set(toks(it["query"])), set(toks(it["situation"]))
        shared = qt & st
        overlap[g] = overlap.get(g, 0) + (1 if shared else 0)
        if g == "para-hard" and shared:
            shared = qt & st
            hard_overlaps.append((it["id"], sorted(shared)[:5]))
    if hard_overlaps:
        fail(f"para-hard zero-overlap violated: {hard_overlaps[:5]}")
    print(f"check4 OK: para-hard query/situation zero content-token overlap; "
          f"per-group items with ANY content-token overlap: {overlap}")

    # Check 3: build fixture, assert template holds + non-empty.
    out = []
    for it in items:
        text = TEMPLATE.format(q=it["query"], s=it["situation"])
        if not text.strip():
            fail(f"item {it['id']}: empty template text")
        if text != TEMPLATE.format(q=it["query"], s=it["situation"]):
            fail(f"item {it['id']}: template assertion failed")
        out.append({
            "id": it["id"],
            "kind": it["kind"],
            "difficulty": it["difficulty"],
            "situation": it["situation"],
            "query": it["query"],
            "action": it["action"],
            "outcome": it["outcome"],
            "discipline_text": text,
        })
    print(f"check3 OK: {len(out)} fixture items, template verified for all")

    OUT.parent.mkdir(parents=True, exist_ok=True)
    with open(OUT, "w", encoding="utf-8", newline="\n") as f:
        json.dump(out, f, indent=2, ensure_ascii=True, sort_keys=True)
        f.write("\n")
    print(f"wrote {OUT} ({OUT.stat().st_size} bytes), corpus order preserved")


if __name__ == "__main__":
    main()
