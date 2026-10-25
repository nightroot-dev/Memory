#!/usr/bin/env python3
"""B12-R save-prompt generator (registered in tools/eval_b8/b12r-save-discipline.md).

Reads the committed corpus, takes the 30 hard items in corpus order, and
writes three batches of 10 incidents per arm:

  tools/eval_b12r/save/w/b01.txt ... b03.txt   (neutral / wild wording)
  tools/eval_b12r/save/d/b01.txt ... b03.txt   (documented discipline)

The files sent to the save sessions are byte-identical to these committed
files. No prompt edits after the pre-registration commit.
"""
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CORPUS = ROOT / "tools" / "corpus_l1.json"
OUT = HERE / "save"

HEADER = (
    "You are recording troubleshooting memories for later reuse.\n"
    "For EACH incident in this file: write the ONE memory text you would save so\n"
    "a future session can find and reuse what was learned. Answer with STRICT\n"
    'JSON mapping every incident id (e.g. "para-006") to its memory text\n'
    "(string). Rules: exactly one entry per incident; keep each memory text to\n"
    "1-2 sentences; do not read any other files; derive the text only from the\n"
    "incident below; no commentary outside the JSON.\n\n"
)

DISCIPLINE = (
    "SAVE DISCIPLINE: In each memory text, include the user's own words for the\n"
    "symptom (the USER SYMPTOM line) alongside what was learned.\n\n"
)


def main() -> int:
    corpus = json.loads(CORPUS.read_text(encoding="utf-8"))
    hard = [x for x in corpus if x["difficulty"] == "hard"]
    assert len(hard) == 30, len(hard)

    for arm in ("w", "d"):
        out_dir = OUT / arm
        out_dir.mkdir(parents=True, exist_ok=True)
        for b in range(0, 30, 10):
            chunk = hard[b : b + 10]
            parts = [HEADER]
            if arm == "d":
                parts.append(DISCIPLINE)
            for x in chunk:
                parts.append(f"INCIDENT {x['id']}")
                parts.append(f"USER SYMPTOM: {x['query']}")
                parts.append(f"WHAT WAS LEARNED: {x['situation']}")
                parts.append("")
            name = f"b{b // 10 + 1:02d}.txt"
            (out_dir / name).write_text("\n".join(parts), encoding="utf-8", newline="\n")
            print(f"written: save/{arm}/{name} ({len(chunk)} incidents)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
