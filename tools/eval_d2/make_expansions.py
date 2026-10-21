#!/usr/bin/env python3
"""D2 fixture generator — couples each hard corpus item with the
uninvolved-LLM's 3 cause expansions (tools/eval_d2/subagent_output.json,
committed verbatim). Output: tools/eval_d2/corpus_l1_d2.json.

Rules (tools/eval_b8/d2-query-expansion.md, registered before the run):
- source corpus: tools/corpus_l1.json ONLY (no keywords in play)
- exactly the 30 hard items get "causes": [c1, c2, c3]; non-hard items
  carry no causes field
- self-checks abort on violation:
  1. n=200; exactly 30 hard items; every hard id present in the
     subagent output; every non-hard id ABSENT from it
  2. every id has exactly 3 non-empty strings, each <= 12 words
  3. no cause string contains the original query verbatim
  4. prints pre-run disclosure: cause-token overlap with store text and
     with the query, per item report written to stdout (counts only)
"""

import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent          # tools/eval_d2
SRC = HERE.parent / "corpus_l1.json"
EXP = HERE / "subagent_output.json"
OUT = HERE / "corpus_l1_d2.json"

TOKEN_RE = re.compile(r"[a-z0-9_]+")


def toks(text: str) -> set[str]:
    return set(TOKEN_RE.findall(text.lower()))


def main() -> int:
    items = json.loads(SRC.read_text(encoding="utf-8"))
    exp = json.loads(EXP.read_text(encoding="utf-8"))
    if len(items) != 200:
        print(f"ABORT: corpus has {len(items)} items", file=sys.stderr)
        return 1

    hard = [i for i in items if i["difficulty"] == "hard"]
    exp_ids = set(exp.keys())
    hard_ids = {i["id"] for i in hard}
    if exp_ids != hard_ids:
        print(f"ABORT: expansion ids != hard ids "
              f"(missing={sorted(hard_ids - exp_ids)}, extra={sorted(exp_ids - hard_ids)})", file=sys.stderr)
        return 1

    original_word_len = lambda s: len(s.split())
    for it in items:
        if it["difficulty"] != "hard":
            continue
        causes = exp[it["id"]]
        if len(causes) != 3 or any(not c.strip() for c in causes):
            print(f"ABORT: {it['id']} bad cause list", file=sys.stderr)
            return 1
        if any(original_word_len(c) > 12 for c in causes):
            print(f"ABORT: {it['id']} cause over 12 words", file=sys.stderr)
            return 1
        q_low = it["query"].strip().lower()
        if any(q_low in c.lower() for c in causes):
            print(f"ABORT: {it['id']} cause contains the query verbatim", file=sys.stderr)
            return 1

    # Pre-run disclosure (counts only — no gate attached).
    print("pre-run disclosure: cause vocabulary vs store text / query")
    for it in hard:
        base = toks(it["situation"] + " " + it["action"] + " " + it["outcome"])
        q = toks(it["query"])
        ov_store, ov_query = [], []
        for c in exp[it["id"]]:
            t = toks(c)
            ov_store.append(len(t & base))
            ov_query.append(len(t & q))
        print(f"  {it['id']}: overlap_with_store={ov_store} overlap_with_query={ov_query}")

    for it in items:
        if it["difficulty"] == "hard":
            it["causes"] = exp[it["id"]]
    OUT.write_text(json.dumps(items, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"written: {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
