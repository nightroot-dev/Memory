#!/usr/bin/env python3
"""Mechanical scorer for the L3b replication sample (see l3b-replication.md).

Usage: python tools/eval_l3/score_repl.py
Reads : pools_record_hard.json, rerank_record_hard_repl.json
Writes: nothing (prints); CI via Clopper-Pearson (exact binomial).
"""
import json
import math
from pathlib import Path

BASE = Path(__file__).resolve().parent
POOLS = BASE / "pools_record_hard.json"
PICKS = BASE / "rerank_record_hard_repl.json"


def cp_ci(k: int, n: int, alpha: float = 0.05):
    """Clopper-Pearson exact interval via bisection on the binomial CDF."""
    lo, hi = 0.0, 1.0
    for _ in range(200):
        mid = (lo + hi) / 2
        # P(X <= k-1 | mid) for lower bound target alpha/2 (k>0)
        cdf = sum(math.comb(n, i) * mid**i * (1 - mid) ** (n - i) for i in range(k)) if k > 0 else 0.0
        if cdf > alpha / 2:
            lo = mid
        else:
            hi = mid
    lower = (lo + hi) / 2 if k > 0 else 0.0

    lo, hi = 0.0, 1.0
    for _ in range(200):
        mid = (lo + hi) / 2
        # P(X <= k | mid) target 1 - alpha/2 for upper bound
        cdf = sum(math.comb(n, i) * mid**i * (1 - mid) ** (n - i) for i in range(k + 1))
        if cdf > 1 - alpha / 2:
            hi = mid
        else:
            lo = mid
    upper = (lo + hi) / 2 if k < n else 1.0
    return lower, upper


def main():
    pools = {it["id"]: it for it in json.loads(POOLS.read_text(encoding="utf-8"))["items"]}
    picks = json.loads(PICKS.read_text(encoding="utf-8"))

    correct = 0
    missing, bad = [], []
    details = []
    for item_id, it in pools.items():
        pick = picks.get(item_id)
        if pick is None:
            missing.append(item_id)
            ok = False
        else:
            # pick must be an int within the presented range
            if not isinstance(pick, int) or not (1 <= pick <= len(it["presented"])):
                bad.append((item_id, pick))
                ok = False
            else:
                ok = it["presented"][pick - 1]["node_id"] == it["gold_node_id"]
        correct += 1 if ok else 0
        details.append((item_id, pick, it["gold_pool_rank"], it["gold_node_id"],
                        it["presented"][pick - 1]["node_id"] if isinstance(pick, int) and 1 <= pick <= len(it["presented"]) else None,
                        ok))

    n = len(pools)
    lo, hi = cp_ci(correct, n)
    print(f"items: {n} | picks present: {n - len(missing)} | missing: {len(missing)} | out-of-range: {len(bad)}")
    print(f"correct: {correct}/{n} = {correct / n:.4f}")
    print(f"Clopper-Pearson 95% CI: [{lo * 100:.1f}%, {hi * 100:.1f}%]")
    print("per-item (id, pick, gold_rank, gold_node, picked_node, ok):")
    for row in details:
        print(" ", row)
    if missing:
        print("MISSING:", missing)
    if bad:
        print("OUT_OF_RANGE:", bad)


if __name__ == "__main__":
    main()
