#!/usr/bin/env python3
"""Mechanical scorer for B12-R (save-discipline -> end-to-end hard recall).

Reads pools_b12r_{w,d}.json (bench dump), picks_{w,d}.json, saved_{w,d}.json,
missing_{w,d}.json, and tools/corpus_l1.json. Prints and writes
tools/eval_b12r/score_b12r.txt.

Pre-registered rules (tools/eval_b8/b12r-save-discipline.md):
- pick N is correct iff presented[N-1].node_id == gold_node_id;
- placeholder items (missing save) are guaranteed misses;
- missing/malformed/out-of-range picks are misses; no repair;
- CP 95% CI via the corrected helper in tools/eval_l3/score_repl.py;
- query-token recall is descriptive (fixed tokenization + stoplist).
"""
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / "tools" / "eval_l3"))
from score_repl import cp_ci  # noqa: E402  (corrected post-verdict helper)

STOP = set(
    """the and for with from after before into over under while when this that
    these those have has had was were are is been being they them their there
    here what which who why how all any both each few more most other some such
    only own same than too very can will just should now not but its you your
    we our us""".split()
)


def tokens(text: str) -> set[str]:
    return {
        t
        for t in re.findall(r"[a-z0-9]+", text.lower())
        if len(t) >= 3 and t not in STOP
    }


def main() -> int:
    corpus = json.loads((ROOT / "tools" / "corpus_l1.json").read_text(encoding="utf-8"))
    hard = [x for x in corpus if x["difficulty"] == "hard"]
    queries = {x["id"]: x["query"] for x in hard}
    assert len(hard) == 30, len(hard)

    lines: list[str] = []
    summary: list[str] = []

    def emit(line: str = "") -> None:
        print(line)
        lines.append(line)

    for arm in ("w", "d"):
        pools = json.loads(
            (ROOT / "tools" / "eval_l3" / f"pools_b12r_{arm}.json").read_text(encoding="utf-8")
        )
        picks = json.loads((HERE / f"picks_{arm}.json").read_text(encoding="utf-8"))
        saved = json.loads((HERE / f"saved_{arm}.json").read_text(encoding="utf-8"))
        missing = set(json.loads((HERE / f"missing_{arm}.json").read_text(encoding="utf-8")))
        assert len(pools["items"]) == 30, len(pools["items"])

        correct = 0
        presented = 0
        recalls: list[float] = []
        rows: list[tuple] = []

        for item in pools["items"]:
            item_id = item["id"]
            if item_id in missing:
                rows.append((item_id, None, item["gold_pool_rank"], None, False, "PLACEHOLDER", 0.0))
                continue
            pick = picks.get(item_id)
            picked = None
            ok = False
            if isinstance(pick, int) and 1 <= pick <= len(item["presented"]):
                picked = item["presented"][pick - 1]["node_id"]
                ok = picked == item["gold_node_id"]
            if item["gold_presented_rank"] is not None:
                presented += 1
            query_tokens = tokens(queries[item_id])
            saved_tokens = tokens(saved.get(item_id, ""))
            recall = len(query_tokens & saved_tokens) / len(query_tokens) if query_tokens else 0.0
            recalls.append(recall)
            if ok:
                correct += 1
            rows.append((item_id, pick, item["gold_pool_rank"], picked, ok, "", recall))

        n = len(pools["items"])
        lo, hi = cp_ci(correct, n)
        non_placeholder = n - len(missing)
        mean_recall = sum(recalls) / len(recalls) if recalls else 0.0

        emit()
        emit(f"== B12-R arm {arm.upper()} ==")
        emit(
            f"correct {correct}/{n} = {correct / n:.4f}; CP95 [{lo * 100:.2f}%, {hi * 100:.2f}%]; "
            f"gold presented {presented}/{non_placeholder} (non-placeholder); "
            f"placeholders {len(missing)}; query-token recall mean {mean_recall:.4f}"
        )
        emit("per-item (id, pick, gold_pool_rank, picked_node, ok, note, query_token_recall):")
        for row in rows:
            emit(f"  {row}")
        summary.append(
            f"{arm.upper()}: {correct}/{n} = {correct / n:.4f} "
            f"CP95 [{lo * 100:.2f}%, {hi * 100:.2f}%] "
            f"presented {presented}/{non_placeholder} placeholders {len(missing)} "
            f"kw-recall {mean_recall:.4f}"
        )

    emit()
    emit("== side by side ==")
    for line in summary:
        emit(line)
    emit()
    emit("Claim rule: D >= 21/30 (70.0%) -> documented save discipline supports the")
    emit("70% point-estimate claim for this regime; below -> no 70% claim.")
    d_correct = int(summary[1].split()[1].split("/")[0])
    emit(f"D verdict: {'PASS' if d_correct >= 21 else 'NO 70% CLAIM'} ({d_correct}/30)")

    (HERE / "score_b12r.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
