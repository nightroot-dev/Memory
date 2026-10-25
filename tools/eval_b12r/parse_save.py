#!/usr/bin/env python3
"""Parse the B12-R save outputs (verbatim per-batch files) into saved_<arm>.json.

Raw files: tools/eval_b12r/save/<arm>/bXX.raw.txt (the model's final text).
Only a single surrounding markdown fence is stripped; no other repair.
Missing/empty/non-string entries are recorded in missing_<arm>.json and become
guaranteed misses (placeholders in the bench). Pre-registered rule; no re-ask.
"""
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CORPUS = ROOT / "tools" / "corpus_l1.json"
OUT = HERE / "save"


def strip_fence(text: str) -> str:
    s = text.strip()
    if s.startswith("```"):
        lines = s.splitlines()
        if lines and lines[0].startswith("```"):
            lines = lines[1:]
        if lines and lines[-1].strip() == "```":
            lines = lines[:-1]
        s = "\n".join(lines).strip()
    return s


def main() -> int:
    corpus = json.loads(CORPUS.read_text(encoding="utf-8"))
    ids = [x["id"] for x in corpus if x["difficulty"] == "hard"]
    assert len(ids) == 30, len(ids)

    for arm in ("w", "d"):
        saved: dict[str, str] = {}
        missing: list[str] = []
        for b in range(0, 30, 10):
            batch_ids = ids[b : b + 10]
            raw_path = OUT / arm / f"b{b // 10 + 1:02d}.raw.txt"
            if not raw_path.exists():
                print(f"{arm} b{b // 10 + 1:02d}: RAW MISSING -> 10 placeholders")
                missing.extend(batch_ids)
                continue
            try:
                obj = json.loads(strip_fence(raw_path.read_text(encoding="utf-8")))
            except Exception as exc:  # noqa: BLE001 - malformed is a registered miss
                print(f"{arm} b{b // 10 + 1:02d}: JSON ERROR ({exc}) -> 10 placeholders")
                missing.extend(batch_ids)
                continue
            if not isinstance(obj, dict):
                print(f"{arm} b{b // 10 + 1:02d}: NOT AN OBJECT -> 10 placeholders")
                missing.extend(batch_ids)
                continue
            for item_id in batch_ids:
                value = obj.get(item_id)
                if isinstance(value, str) and value.strip():
                    saved[item_id] = value.strip()
                else:
                    missing.append(item_id)

        (HERE / f"saved_{arm}.json").write_text(
            json.dumps(saved, indent=1) + "\n", encoding="utf-8"
        )
        (HERE / f"missing_{arm}.json").write_text(
            json.dumps(sorted(set(missing)), indent=1) + "\n", encoding="utf-8"
        )
        print(f"{arm}: saved={len(saved)} missing={len(set(missing))} -> {sorted(set(missing))}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
