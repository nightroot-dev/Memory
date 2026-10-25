#!/usr/bin/env python3
"""B12-R pick-prompt generator (registered before the pick run).

Reads the committed pool dumps written by the bench test
(b12r_dump_agent_texts) and writes three batches of 10 items per arm:

  tools/eval_b12r/pick/w/b01.txt ... b03.txt
  tools/eval_b12r/pick/d/b01.txt ... b03.txt

Header and item layout are byte-identical to the L3b pick batches. Gold
fields are never emitted. The files sent to the picker are byte-identical
to these committed files.
"""
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
POOLS = ROOT / "tools" / "eval_l3"

HEADER = (
    "You are an uninvolved assistant reranking stored troubleshooting memories.\n"
    "For EACH item in this file: pick the ONE candidate memory that best explains\n"
    "the user's symptom. Answer with STRICT JSON mapping every item id to its\n"
    "chosen candidate NUMBER (integer). Rules: choose only from the presented\n"
    "candidate numbers of that item; exactly one answer per item; no abstentions;\n"
    "do not read any other files; derive the answer only from the text below.\n\n"
)

LEGS = (("w", "pools_b12r_w.json"), ("d", "pools_b12r_d.json"))


def main() -> int:
    for arm, dump_name in LEGS:
        dump = json.loads((POOLS / dump_name).read_text(encoding="utf-8"))
        assert dump["cap"] == 20, f"unexpected cap {dump['cap']}"
        assert len(dump["items"]) == 30, f"{arm}: {len(dump['items'])} items"
        out_dir = HERE / "pick" / arm
        out_dir.mkdir(parents=True, exist_ok=True)
        for b in range(0, len(dump["items"]), 10):
            chunk = dump["items"][b : b + 10]
            parts = [HEADER]
            for it in chunk:
                parts.append(f"ITEM {it['id']}")
                parts.append(f"USER SYMPTOM: {it['query']}")
                parts.append("CANDIDATES:")
                for cand in it["presented"]:
                    parts.append(f"{cand['rank']}. {cand['text']}")
                parts.append("")
            name = f"b{b // 10 + 1:02d}.txt"
            (out_dir / name).write_text("\n".join(parts), encoding="utf-8", newline="\n")
            print(f"written: pick/{arm}/{name} ({len(chunk)} items)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
