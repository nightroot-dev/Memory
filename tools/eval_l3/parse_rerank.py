#!/usr/bin/env python3
"""Parse the verbatim subagent rerank outputs (raw text, committed as .raw)
into per-leg JSON maps {item-id: chosen number}; the scorer consumes these.
Validates completeness against the pool dumps (registered: every dump item
must have a pick; range violations are NOT fixed here — the scorer counts
them as lost_rank_rerank per pre-registration)."""

import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent

RAW = {
    "record_hard": ("rerank_record_hard.raw", "pools_record_hard.json", 30),
    "d2expanded": ("rerank_d2expanded.raw", "pools_d2expanded_para.json", 100),
    "w1bridge": ("rerank_w1bridge.raw", "pools_w1bridge_all.json", 100),
}


def parse_raw(text: str) -> dict[str, int]:
    picks: dict[str, int] = {}
    # lines like:  b01.txt: {"para-006": 11, ...}   (b01/b01.txt both accepted)
    for line in text.splitlines():
        if ":" not in line:
            continue
        left, _, right = line.partition(":")
        left = left.strip()
        if not re.fullmatch(r"b\d{2}(\.txt)?", left):
            continue
        obj = json.loads(right.strip())
        for k, v in obj.items():
            picks[k] = int(v)
    return picks


def main() -> int:
    ok = True
    for leg, (raw_name, dump_name, expected) in RAW.items():
        text = (HERE / raw_name).read_text(encoding="utf-8")
        picks = parse_raw(text)
        dump = json.loads((HERE / dump_name).read_text(encoding="utf-8"))
        ids = [it["id"] for it in dump["items"]]
        missing = [i for i in ids if i not in picks]
        dupes = len(picks) - len(set(picks) & set(ids))
        (HERE / f"rerank_{leg}.json").write_text(
            json.dumps(picks, indent=1) + "\n", encoding="utf-8"
        )
        status = "OK" if not missing and len(picks) == expected and dupes == 0 else "BAD"
        if status == "BAD":
            ok = False
        print(f"{leg}: picks={len(picks)} expected={expected} missing={missing} foreign={dupes} -> {status}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
