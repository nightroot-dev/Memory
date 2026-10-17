#!/usr/bin/env python3
"""B12 session-2 verifier. PRE-REGISTERED rubric; never tuned after runs.

Usage: python tools/eval_b12/verify.py <task> <report-s2.md> <run-workdir>
Exit 0 = session-2 PASS. Prints each check + a final verdict.

Rubric per task:
- files: (relpath, regex) every entry must match in the run work dir.
- order: (relpath, before_regex, after_regex) first index-of requires before < after.
- forbid: fixture files that must be byte-identical to the generator output.
- report: list of keyword groups; EVERY group needs at least one hit in report md.
"""
import hashlib, pathlib, re, sys

ROOT = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT))
from make_fixtures import TASKS  # canonical fixture bytes (committed generator)

RUBRICS = {
 "t1": dict(
   files=[("sched/NightSync.xml", r"sync-nightly\.ps1"),
          ("conf/sync.toml", r"enabled\s*=\s*true"),
          ("ops/fix-log.txt", r"NightSync\.xml"),
          ("ops/fix-log.txt", r"sync-nightly\.ps1"),
          ("ops/fix-log.txt", r"enabled\s*=\s*true")],
   forbid=["scripts/sync-nightly.ps1", "logs/sync.log"],
   report=[["fix-log"], ["enabled"], ["sync-nightly"]]),
 "t2": dict(
   files=[("env/prod.ini", r"mode\s*=\s*PROD"),
          ("env/prod.ini", r"maxconn\s*=\s*250"),
          ("app/load.py", r"--env"),
          ("app/load.py", r"prod\.ini")],
   forbid=["app/app.ini", "env/overrides.ini"],
   report=[["prod.ini"], ["include"]]),
 "t3": dict(
   files=[("docs/README.md", r"GRID\s*=\s*4\b"),
          ("tools/changelog.txt", r"flags doc synced to calctl\.py")],
   forbid=["tools/calctl.py"],
   report=[["GRID"], ["4"]]),
 "t4": dict(
   files=[("build/build-note.txt", r"precedence|placement|include")],
   order=[("Makefile", r"include config/defaults\.mk", r"\bbuild:")],
   forbid=["config/defaults.mk", "src/main.c"],
   report=[["include"], ["defaults.mk"]]),
 "t5": dict(
   files=[("scripts/loadenv.ps1", r"local\.env"),
          ("dev/env-fix.txt", r"local\.env")],
   order=[("scripts/loadenv.ps1", r"local\.env", r"common\.env"),
          ("dev/env-fix.txt", r"local\.env", r"common\.env")],
   forbid=["env/common.env", "env/local.env", "src/run_app.ps1"],
   report=[["order", "before", "sourced"], ["local.env"]]),
 "t6": dict(
   files=[("db/migrations/003_noop_backfill.sql", r"-- no-op backfill"),
          ("db/mig-note.txt", r"gap|numbering|consecutive")],
   forbid=["db/migrations/001_init.sql", "db/migrations/002_add_days.sql",
           "db/migrations/004_add_tag.sql", "db/schema_migrations.csv", "db/migrate.ps1"],
   report=[["003"], ["gap|missing|consecutive"]]),
 "t7": dict(
   files=[("routes/reorder-note.txt", r"/static"),
          ("routes/app.routes", r"/health\s+->\s+health\.handler")],
   order=[("routes/app.routes", r"/static/\*", r"/blog/\*")],
   forbid=["docs/routing.txt", "logs/access.log"],
   report=[["shadow", "order"], ["static"]]),
 "t8": dict(
   files=[("pkg/pyproject.toml", r"data-jobs\s*="),
          ("pkg/pyproject.toml", r"datajob\s*="),
          ("pkg/rename-note.txt", r"data-jobs"),
          ("pkg/rename-note.txt", r"datajob")],
   forbid=["docs/README.txt", "pkg/repro.txt"],
   report=[["pyproject|scripts"], ["data-jobs"]]),
 "t9": dict(
   files=[("logs/summary.txt", r"E-PAY-1183"),
          ("logs/summary.txt", r"app-3"),
          ("logs/summary.txt", r"21:37")],
   forbid=["logs/app-1.log", "logs/app-2.log", "logs/app-3.log", "logs/app-4.log", "logs/app-5.log"],
   report=[["E-PAY-1183"], ["app-3"], ["21:37"]]),
 "t10": dict(
   files=[("req/drift.txt", r"notice-utils"),
          ("req/drift.txt", r"1\.2\.3"),
          ("req/drift.txt", r"1\.2\.4"),
          ("scripts/install.sh", r"# FIRM: notice-utils==1\.2\.3")],
   forbid=["req/pin.lock", "req/requirements.txt", "docs/audit.txt"],
   report=[["1.2.3"], ["1.2.4"]]),
}

def sha(b): return hashlib.sha256(b).hexdigest()

def fixture_bytes(task_id):
    t = next(t for t in TASKS if t['id'] == task_id)
    return t['files']

def main() -> int:
    try:
        task, report_path, workdir = sys.argv[1], sys.argv[2], pathlib.Path(sys.argv[3])
    except IndexError:
        print("usage: verify.py <task> <report.md> <workdir>"); return 2
    if task not in RUBRICS:
        print(f"no rubric for {task}"); return 2
    rub = RUBRICS[task]
    failures = []

    def read(rel):
        p = workdir / rel
        if not p.exists():
            failures.append(f"MISSING FILE: {rel}"); return None
        return p.read_text(encoding="utf-8", errors="replace")

    for rel, pat in rub["files"]:
        txt = read(rel)
        if txt is not None and not re.search(pat, txt):
            failures.append(f"REGEX MISS: {rel} ~ /{pat}/")
    for rel, before, after in rub.get("order", []):
        txt = read(rel)
        if txt is not None:
            m1, m2 = re.search(before, txt), re.search(after, txt)
            if not m1 or not m2 or m1.start() >= m2.start():
                failures.append(f"ORDER MISS: {rel} /{before}/ must precede /{after}/")
    canon = fixture_bytes(task)
    for rel in rub["forbid"]:
        p = workdir / rel
        if not p.exists():
            failures.append(f"FORBIDDEN FILE GONE: {rel}"); continue
        want = sha(canon[rel].encode("utf-8"))
        got = sha(p.read_bytes())
        if want != got:
            failures.append(f"FORBIDDEN EDIT: {rel} (changed)")
    rep = pathlib.Path(report_path)
    rep_txt = rep.read_text(encoding="utf-8", errors="replace") if rep.exists() else ""
    if not rep_txt.strip():
        failures.append("REPORT EMPTY/MISSING")
    for group in rub["report"]:
        if not any(re.search(k, rep_txt, re.I) for k in group):
            failures.append(f"REPORT GROUP MISS: {group}")

    if failures:
        print(f"[{task}] FAIL ({len(failures)} checks)")
        for f in failures:
            print("  -", f)
        return 1
    print(f"[{task}] PASS (all file-state, order, tamper and report checks)")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
