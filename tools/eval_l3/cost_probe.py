#!/usr/bin/env python3
import json
import statistics
from pathlib import Path

base = Path(r"C:\Games\Projects\Memory\tools\eval_l3")
CH = 4  # ~chars per token, English prose

legs = [
    ("pools_w1bridge_all.json", "L3d (ship stack)"),
    ("pools_d2expanded_para.json", "L3c"),
    ("pools_record_hard.json", "L3b"),
]
for dump_name, leg in legs:
    d = json.loads((base / dump_name).read_text(encoding="utf-8"))
    per = []
    for it in d["items"]:
        block = len(it["query"]) + 20 + sum(len(c["text"]) + len(str(c["rank"])) + 3 for c in it["presented"])
        per.append(block)
    total = sum(per)
    print(f"{leg}: n={len(per)} | per-item chars mean={statistics.mean(per):.0f} median={statistics.median(per):.0f} max={max(per)}")
    print(f"   per-item approx tokens: mean~{statistics.mean(per)/CH:.0f} median~{statistics.median(per)/CH:.0f} worst~{max(per)/CH:.0f} | whole-leg input ~{total/CH:.0f} tokens")

for leg in ["l3b", "l3c", "l3d"]:
    files = sorted((base / "batches" / leg).glob("b*.txt"))
    chars = sum(f.stat().st_size for f in files)
    n = 30 if leg == "l3b" else 100
    print(f"{leg}: {len(files)} committed batch files, {chars} chars -> ~{chars/CH:.0f} tokens REAL LLM input for {n} items")

out = sum((base / f"rerank_{n}.raw").stat().st_size for n in ["record_hard", "d2expanded", "w1bridge"])
print(f"rerank LLM outputs total: {out} chars -> ~{out/CH:.0f} tokens for all 230 picks")
