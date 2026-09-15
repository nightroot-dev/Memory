#!/usr/bin/env python3
"""
analyze_gate_shadow.py - Dual-schema parser & calibration analyzer for gate_shadow_log.jsonl.

Supports:
  - Schema v=1: Legacy shadow log (policy_v=1, implicit verifiability=1.0, threshold=0.35, enforced=false)
  - Schema v=2: Policy v2 shadow log (explicit verifiability, threshold, policy_v=2, enforced)

Computes empirical distributions of max_cosine, novelty, prediction_error,
verifiability, and gate_score, along with parametric threshold sensitivity curves.
"""

import argparse
import json
import math
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple


def parse_line(raw: str, default_threshold: float = 0.35) -> Optional[Dict[str, Any]]:
    raw = raw.strip()
    if not raw:
        return None
    try:
        entry = json.loads(raw)
    except Exception:
        return None

    v = entry.get("v", 1)
    if v == 1:
        return {
            "v": 1,
            "policy_v": entry.get("policy_v", 1),
            "event_id": entry.get("event_id", ""),
            "session_id": entry.get("session_id", ""),
            "ctx_id": entry.get("ctx_id", ""),
            "ts": entry.get("ts", 0),
            "model": entry.get("model", "unknown"),
            "max_cosine": float(entry.get("max_cosine", 0.0)),
            "novelty": float(entry.get("novelty", 1.0)),
            "prediction_error": float(entry.get("prediction_error", 0.0)),
            "verifiability": 1.0,
            "gate_score": float(entry.get("gate_score", 0.0)),
            "threshold": default_threshold,
            "would_index": bool(entry.get("would_index", True)),
            "enforced": False,
        }
    elif v == 2:
        return {
            "v": 2,
            "policy_v": entry.get("policy_v", 2),
            "event_id": entry.get("event_id", ""),
            "session_id": entry.get("session_id", ""),
            "ctx_id": entry.get("ctx_id", ""),
            "ts": entry.get("ts", 0),
            "model": entry.get("model", "unknown"),
            "max_cosine": float(entry.get("max_cosine", 0.0)),
            "novelty": float(entry.get("novelty", 1.0)),
            "prediction_error": float(entry.get("prediction_error", 0.0)),
            "verifiability": float(entry.get("verifiability", 1.0)),
            "gate_score": float(entry.get("gate_score", 0.0)),
            "threshold": float(entry.get("threshold", default_threshold)),
            "would_index": bool(entry.get("would_index", True)),
            "enforced": bool(entry.get("enforced", False)),
        }
    else:
        # Forward compatible: parse what exists
        return {
            "v": v,
            "policy_v": entry.get("policy_v", 2),
            "event_id": entry.get("event_id", ""),
            "session_id": entry.get("session_id", ""),
            "ctx_id": entry.get("ctx_id", ""),
            "ts": entry.get("ts", 0),
            "model": entry.get("model", "unknown"),
            "max_cosine": float(entry.get("max_cosine", 0.0)),
            "novelty": float(entry.get("novelty", 1.0)),
            "prediction_error": float(entry.get("prediction_error", 0.0)),
            "verifiability": float(entry.get("verifiability", 1.0)),
            "gate_score": float(entry.get("gate_score", 0.0)),
            "threshold": float(entry.get("threshold", default_threshold)),
            "would_index": bool(entry.get("would_index", True)),
            "enforced": bool(entry.get("enforced", False)),
        }


def stats(values: List[float]) -> Dict[str, float]:
    if not values:
        return {"n": 0, "min": 0.0, "p10": 0.0, "p25": 0.0, "p50": 0.0, "p75": 0.0, "p90": 0.0, "max": 0.0, "mean": 0.0, "std": 0.0}
    s = sorted(values)
    n = len(s)
    mean = sum(s) / n
    var = sum((x - mean) ** 2 for x in s) / n if n > 1 else 0.0

    def pct(p: float) -> float:
        idx = int(round(p * (n - 1)))
        return s[max(0, min(n - 1, idx))]

    return {
        "n": n,
        "min": s[0],
        "p10": pct(0.10),
        "p25": pct(0.25),
        "p50": pct(0.50),
        "p75": pct(0.75),
        "p90": pct(0.90),
        "max": s[-1],
        "mean": mean,
        "std": math.sqrt(var),
    }


def analyze_log(log_path: Path, default_threshold: float = 0.35) -> Dict[str, Any]:
    if not log_path.is_file():
        raise FileNotFoundError(f"Log file not found: {log_path}")

    records = []
    v1_count = 0
    v2_count = 0
    unrecognized = 0

    with open(log_path, "r", encoding="utf-8") as f:
        for line in f:
            rec = parse_line(line, default_threshold)
            if rec is None:
                unrecognized += 1
                continue
            records.append(rec)
            if rec["v"] == 1:
                v1_count += 1
            elif rec["v"] == 2:
                v2_count += 1

    total = len(records)
    if total == 0:
        return {"total_records": 0, "v1_records": 0, "v2_records": 0, "status": "empty"}

    scores = [r["gate_score"] for r in records]
    novelties = [r["novelty"] for r in records]
    pes = [r["prediction_error"] for r in records]
    verifs = [r["verifiability"] for r in records]
    cosines = [r["max_cosine"] for r in records]

    passed_count = sum(1 for r in records if r["would_index"])
    pass_rate = passed_count / total

    # Sensitivity curve across potential thresholds
    test_thresholds = [0.15, 0.20, 0.25, 0.30, 0.35, 0.40, 0.45, 0.50, 0.60]
    sweep = {}
    for t in test_thresholds:
        sim_pass = sum(1 for s in scores if s >= t)
        sweep[f"{t:.2f}"] = {
            "admitted": sim_pass,
            "suppressed": total - sim_pass,
            "admission_rate": round(sim_pass / total, 4),
        }

    # Breakdown by verifiability tiers
    verif_tiers = [
        ("0.0 - 0.2", lambda v: v < 0.2),
        ("0.2 - 0.4", lambda v: 0.2 <= v < 0.4),
        ("0.4 - 0.6", lambda v: 0.4 <= v < 0.6),
        ("0.6 - 0.8", lambda v: 0.6 <= v < 0.8),
        ("0.8 - 1.0", lambda v: v >= 0.8),
    ]
    tier_stats = {}
    for label, pred in verif_tiers:
        tier_recs = [r for r in records if pred(r["verifiability"])]
        t_total = len(tier_recs)
        if t_total > 0:
            t_pass = sum(1 for r in tier_recs if r["gate_score"] >= default_threshold)
            tier_stats[label] = {
                "count": t_total,
                "admitted": t_pass,
                "suppressed": t_total - t_pass,
                "admission_rate": round(t_pass / t_total, 4),
                "mean_score": round(sum(r["gate_score"] for r in tier_recs) / t_total, 4),
            }
        else:
            tier_stats[label] = {"count": 0, "admitted": 0, "suppressed": 0, "admission_rate": 0.0, "mean_score": 0.0}

    return {
        "total_records": total,
        "v1_records": v1_count,
        "v2_records": v2_count,
        "unrecognized_lines": unrecognized,
        "observed_admission_rate": round(pass_rate, 4),
        "distributions": {
            "gate_score": stats(scores),
            "novelty": stats(novelties),
            "prediction_error": stats(pes),
            "verifiability": stats(verifs),
            "max_cosine": stats(cosines),
        },
        "threshold_sensitivity_sweep": sweep,
        "verifiability_tier_analysis": tier_stats,
    }


def format_table(d: Dict[str, float]) -> str:
    return (
        f"n={d['n']} | min={d['min']:.3f} | p25={d['p25']:.3f} | p50={d['p50']:.3f} | "
        f"p75={d['p75']:.3f} | p90={d['p90']:.3f} | max={d['max']:.3f} | "
        f"mean={d['mean']:.3f}±{d['std']:.3f}"
    )


def print_report(res: Dict[str, Any], path: str):
    print("=" * 80)
    print(f"GATE SHADOW LOG CALIBRATION ANALYSIS: {path}")
    print("=" * 80)
    print(f"Total Records: {res['total_records']} (Schema v1: {res['v1_records']}, Schema v2: {res['v2_records']})")
    print(f"Observed Admission Rate: {res['observed_admission_rate']*100:.1f}%")
    print("-" * 80)
    print("EMPIRICAL DISTRIBUTIONS:")
    for metric, dist in res["distributions"].items():
        print(f"  {metric:<18}: {format_table(dist)}")
    print("-" * 80)
    print("THRESHOLD SENSITIVITY SWEEP (Hypothetical Admission Rates):")
    print("  Threshold | Admitted | Suppressed | Admission Rate")
    for t_str, sweep_res in res["threshold_sensitivity_sweep"].items():
        bar = "#" * int(sweep_res["admission_rate"] * 20)
        print(f"    {t_str:<7} | {sweep_res['admitted']:<8} | {sweep_res['suppressed']:<10} | {sweep_res['admission_rate']*100:5.1f}%  {bar}")
    print("-" * 80)
    print("VERIFIABILITY TIER ADMISSION ANALYSIS (at default threshold):")
    print("  Tier (Verif) | Count | Admitted | Suppressed | Admission Rate | Mean Score")
    for tier, t_stat in res["verifiability_tier_analysis"].items():
        print(f"   {tier:<11} | {t_stat['count']:<5} | {t_stat['admitted']:<8} | {t_stat['suppressed']:<10} | {t_stat['admission_rate']*100:5.1f}%          | {t_stat['mean_score']:.3f}")
    print("=" * 80)


def main():
    parser = argparse.ArgumentParser(description="Dual-schema gate_shadow_log analyzer.")
    parser.add_argument("log_file", type=str, help="Path to gate_shadow_log.jsonl")
    parser.add_argument("--threshold", type=float, default=0.35, help="Gate threshold (default: 0.35)")
    parser.add_argument("--json", action="store_true", help="Output raw JSON instead of text report")
    args = parser.parse_args()

    log_path = Path(args.log_file)
    try:
        results = analyze_log(log_path, args.threshold)
    except Exception as e:
        print(f"Error analyzing log: {e}", file=sys.stderr)
        sys.exit(1)

    if args.json:
        print(json.dumps(results, indent=2))
    else:
        print_report(results, str(log_path))


if __name__ == "__main__":
    main()
