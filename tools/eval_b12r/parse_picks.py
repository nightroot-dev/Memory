#!/usr/bin/env python3
"""Parse the B12-R pick outputs (verbatim per-batch files) into picks_<arm>.json.

Raw files: tools/eval_b12r/pick/<arm>/bXX.raw.txt (the picker's final text).
Only a single surrounding markdown fence is stripped. Missing/malformed/
out-of-range picks are NOT repaired here; the scorer counts them as misses
(pre-registered).
"""
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


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
    for arm in ("w", "d"):
        picks: dict[str, int] = {}
        for b in range(1, 4):
            raw_path = HERE / "pick" / arm / f"b{b:02d}.raw.txt"
            if not raw_path.exists():
                print(f"{arm} b{b:02d}: RAW MISSING -> those picks are misses")
                continue
            try:
                obj = json.loads(strip_fence(raw_path.read_text(encoding="utf-8")))
            except Exception as exc:  # noqa: BLE001 - malformed is a registered miss
                print(f"{arm} b{b:02d}: JSON ERROR ({exc}) -> those picks are misses")
                continue
            if not isinstance(obj, dict):
                print(f"{arm} b{b:02d}: NOT AN OBJECT -> those picks are misses")
                continue
            for key, value in obj.items():
                try:
                    picks[str(key)] = int(value)
                except (TypeError, ValueError):
                    print(f"{arm} b{b:02d}: non-integer pick for {key!r} ignored (miss)")
        (HERE / f"picks_{arm}.json").write_text(
            json.dumps(picks, indent=1) + "\n", encoding="utf-8"
        )
        print(f"{arm}: picks={len(picks)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
