#!/usr/bin/env python3
"""B12 arm-session runner (pilot: t1 only).

Runs ONE arm session end-to-end:
  python tools/eval_b12/run_arm.py <arm> <s1|s2>
where arm in {none, self-notes, cmhm}.

Mechanics (documented, deterministic):
- session 1: fresh copy of fixtures into the arm run dir (done by this script
  only when --prepare is passed or the run dir does not exist yet).
- prompt = RUNNER NOTE (identical across arms) + session body (verbatim from
  tasks/b12-t1/task.md) + arm block (verbatim from the same file).
- spawn: opencode run --model bench-agent-m --agent general --auto --title <title>
  cwd = C:\\Games\\Projects\\Memory (cmhm MCP config needs the project cwd).
- records NOTHING into the repo; only the run dir gets prompt audit file.
- report-s<k>.md is extracted LATER from opencode.db by metrics.py.

S-protocol: the runner makes NO memory calls itself. cmhm arm tripwires are
done by tripwire.py around the cmhm arm, not here.
"""
import pathlib
import re
import shutil
import subprocess
import sys

REPO = pathlib.Path(r"C:\Games\Projects\Memory")
OC = r"C:\Users\dbroot\AppData\Roaming\npm\opencode.cmd"
TASK = REPO / "tools" / "eval_b12" / "tasks" / "b12-t1"
RUNS = REPO / "tools" / "eval_b12" / "runs" / "b12-t1"
MODEL = "bench-agent-m-WITHHELD#xhigh"
AGENT = "general"
BODY_S1 = "## SESSION 1 PROMPT (body used verbatim for every arm)"
BODY_S2 = "## SESSION 2 PROMPT (body used verbatim for every arm)"


def body(kind: str) -> str:
    text = (TASK / "task.md").read_text(encoding="utf-8")
    start = text.index(BODY_S2 if kind == "s2" else BODY_S1)
    # body ends at the next "## " heading after the start
    nxt = text.find("\n## ", start + 10)
    seg = text[start + len(BODY_S1 if kind == "s1" else BODY_S2): nxt if nxt != -1 else len(text)]
    return seg.strip("\n")


def arm_block(arm: str, kind: str) -> str:
    text = (TASK / "task.md").read_text(encoding="utf-8")
    anchor = "## SESSION-2 ARM BLOCKS" if kind == "s2" else "## SESSION-1 ARM BLOCKS"
    nxt = text.find("\n## ", text.index(anchor) + 4)
    seg = text[text.index(anchor): nxt if nxt != -1 else len(text)]
    if arm == "none":
        # none arm: arm block = no extra instruction (README §16 "none — prompt
        # only"; the task.md none row is bookkeeping, not content).
        return ""
    key = "self-notes" if arm == "self-notes" else "cmhm"
    marker = f"- {key}:"
    i = seg.index(marker)
    val = seg[i + len(marker):].strip().splitlines()[0].strip()
    val = val.replace(f"ARM BLOCK ({key}): ", "", 1)
    return val


def main() -> int:
    arm, kind = sys.argv[1], sys.argv[2]
    assert arm in ("none", "self-notes", "cmhm"), arm
    assert kind in ("s1", "s2"), kind
    run_dir = RUNS / arm
    if kind == "s1":
        # fresh workspace per arm: wipe any stale run dir, copy fixtures
        if run_dir.exists():
            shutil.rmtree(run_dir)
        shutil.copytree(TASK / "fixtures", run_dir)
    run_dir.mkdir(parents=True, exist_ok=True)

    note = (
        f"RUNNER NOTE (identical wording for every arm): Your work directory "
        f"for this session is {run_dir}. Create, read and edit files ONLY "
        f"inside it.\n\n"
    )
    prompt = note + body(kind) + "\n\n" + arm_block(arm, kind)
    (run_dir / f"prompt-{kind}.txt").write_text(prompt, encoding="utf-8", newline="")

    title = f"B12 t1 {arm} {kind}"
    cmd = [OC, "run", "--model", MODEL, "--agent", AGENT, "--auto",
           "--title", title, prompt]
    print(f"[run] {title} -> cwd={REPO}")
    proc = subprocess.run(cmd, cwd=str(REPO), capture_output=True, text=True,
                          encoding="utf-8", errors="replace", timeout=1800)
    (run_dir / f"cli-{kind}.log").write_text(
        f"exit={proc.returncode}\n--STDOUT--\n{proc.stdout}\n--STDERR--\n{proc.stderr}",
        encoding="utf-8", newline="")
    print(f"[done] exit={proc.returncode} (log: cli-{kind}.log, {len(proc.stdout)} chars stdout)")
    return proc.returncode


if __name__ == "__main__":
    sys.exit(main())
