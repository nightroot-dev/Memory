"""B13 scoring side-artifact: CP interval for the discipline legs, using the
standing corrected CP helper (tools/eval_l3/score_repl.py) per
tools/eval_b8/b13-ship-default.md §7 (standing CP discipline)."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools" / "eval_l3"))
from score_repl import cp_ci

if __name__ == "__main__":
    lo, hi = cp_ci(29, 30)
    print(f"hard discipline legs 29/30: [{lo:.2%}, {hi:.2%}] (CP 95)")
    lo2, hi2 = cp_ci(30, 30)
    print(f"reference b12-r D 30/30:    [{lo2:.2%}, {hi2:.2%}] (CP 95)")
