#!/usr/bin/env python3
"""Emit validated pairs as Rust tuple lines."""
import json
from pathlib import Path

pairs = json.loads((Path(__file__).resolve().parent / "validated_pairs.json").read_text())
print(f"pairs: {len(pairs)}")
lines = []
for p in pairs:
    s = p["s"].replace('"', "'")
    q = p["q"].replace('"', "'")
    lines.append(f'        ("{s}", "{q}"),')
Path("pairs_rust.txt").write_text("\n".join(lines) + "\n")
print("\n".join(lines[:6]))
