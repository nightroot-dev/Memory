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


def _cdf(x: int, n: int, p: float) -> float:
    """P(X <= x) for X ~ Binomial(n, p)."""
    if x < 0:
        return 0.0
    if x >= n:
        return 1.0
    return sum(math.comb(n, i) * p**i * (1 - p) ** (n - i) for i in range(x + 1))


def cp_ci(k: int, n: int, alpha: float = 0.05):
    """Clopper-Pearson exact interval via bisection on the binomial CDF.

    Lower bound solves P(X <= k-1 | p) = 1 - alpha/2; upper bound solves
    P(X <= k | p) = alpha/2. Post-verdict correction 2026-10-25: the
    originally committed helper had both bisection comparisons inverted and
    returned an interval excluding the point estimate. Picks and the pick
    rule were never affected; see tools/eval_b8/l3b-replication.md PART 2.
    """
    lo, hi = 0.0, 1.0
    for _ in range(200):
        mid = (lo + hi) / 2
        if _cdf(k - 1, n, mid) > 1 - alpha / 2:
            lo = mid
        else:
            hi = mid
    lower = (lo + hi) / 2 if k > 0 else 0.0

    lo, hi = 0.0, 1.0
    for _ in range(200):
        mid = (lo + hi) / 2
        if _cdf(k, n, mid) > alpha / 2:
            lo = mid
        else:
            hi = mid
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
