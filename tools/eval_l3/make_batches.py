#!/usr/bin/env python3
"""L3 rerank prompt builder — reads tools/eval_l3/pools_*.json ( dumps; they
contain gold fields, which are STRIPPED here) and writes per-leg batch prompt
files (10 items each) to:
  - tools/eval_l3/batches/<leg>/bXX.txt        (committed provenance copies)
  - <temp>/l3_batches/<leg>/bXX.txt            (byte-identical copies the
    uninvolved LLM is allowed to read; the committed tree stays ring-fenced)

Prompt content per item (registered): item id, USER SYMPTOM (query), numbered
candidate texts (<= 20, presentation order), no gold labels, no action/outcome
text. The LLM answers STRICT JSON {item-id: candidate-number} per file.
"""

import json
import shutil
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent          # tools/eval_l3
TEMP = Path(r"C:\Users\dbroot\AppData\Local\Temp\opencode\l3_batches")

LEGS = [
    ("b", "pools_record_hard.json", 30),
    ("c", "pools_d2expanded_para.json", 100),
    ("d", "pools_w1bridge_all.json", 100),
]

HEADER = (
    "You are an uninvolved assistant reranking stored troubleshooting memories.\n"
    "For EACH item in this file: pick the ONE candidate memory that best explains\n"
    "the user's symptom. Answer with STRICT JSON mapping every item id to its\n"
    "chosen candidate NUMBER (integer). Rules: choose only from the presented\n"
    "candidate numbers of that item; exactly one answer per item; no abstentions;\n"
    "do not read any other files; derive the answer only from the text below.\n\n"
)


def main() -> int:
    if TEMP.exists():
        shutil.rmtree(TEMP)
    batch_manifest = {}
    for leg, dump_name, expected in LEGS:
        dump = json.loads((HERE / dump_name).read_text(encoding="utf-8"))
        assert dump["cap"] == 20, f'unexpected cap {dump["cap"]}'
        assert len(dump["items"]) == expected, f"{leg}: {len(dump['items'])} != {expected}"
        out_dir = HERE / "batches" / f"l3{leg}"
        temp_dir = TEMP / f"l3{leg}"
        out_dir.mkdir(parents=True, exist_ok=True)
        temp_dir.mkdir(parents=True, exist_ok=True)
        files = []
        items = dump["items"]
        for b in range(0, len(items), 10):
            chunk = items[b : b + 10]
            parts = [HEADER]
            for it in chunk:
                parts.append(f"ITEM {it['id']}")
                parts.append(f"USER SYMPTOM: {it['query']}")
                parts.append("CANDIDATES:")
                for c in it["presented"]:
                    parts.append(f"{c['rank']}. {c['text']}")
                parts.append("")
            content = "\n".join(parts)
            name = f"b{b // 10 + 1:02d}.txt"
            (out_dir / name).write_text(content, encoding="utf-8", newline="\n")
            (temp_dir / name).write_text(content, encoding="utf-8", newline="\n")
            files.append(name)
        batch_manifest[f"l3{leg}"] = {
            "dump": dump_name,
            "files": files,
            "items": len(items),
            "temp_dir": str(temp_dir),
        }
    (HERE / "batch_manifest.json").write_text(
        json.dumps(batch_manifest, indent=2) + "\n", encoding="utf-8"
    )
    for leg, info in batch_manifest.items():
        print(f"{leg}: {len(info['files'])} files -> {info['temp_dir']}")
    print("written: batch_manifest.json")
    return 0


if __name__ == "__main__":
    sys.exit(main())
