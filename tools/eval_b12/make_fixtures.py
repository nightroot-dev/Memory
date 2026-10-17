#!/usr/bin/env python3
"""B12 fixture + task.md generator (deterministic; re-run is a no-op if unchanged).

Writes tools/eval_b12/tasks/b12-t<k>/task.md and fixtures/. Content is final
BEFORE any arm run (design gate). Offline; no randomness; byte-stable.
"""
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent

# Verbatim arm blocks (single source; README quotes these too).
ARM_NONE_S1 = "ARM BLOCK (none): none - just work in this directory."
ARM_NOTES_S1 = (
    "ARM BLOCK (self-notes): As you work, maintain a file named NOTES.md in your work directory. "
    "Write down every fact you learn that would be needed to resume or redo this task later: root causes, file locations, commands, pitfalls, decisions. "
    "Keep it concise but complete. Do not write anything outside your work directory."
)
ARM_CMHM_S1 = (
    "ARM BLOCK (cmhm): You have the CMHM memory server. First call context_create with display_name "
    "{ctx} (context_type 'session') and use ONLY that context for all memory calls. Learn outcome-changing "
    "events as you work (per the server instructions; do not spam). Retrievals in this session: at most 2."
)
ARM_CMHM_S2 = (
    "ARM BLOCK (cmhm): A previous session on this exact task used the CMHM memory server. Call context_list FIRST, "
    "find the context whose display_name starts with {ctx} (ignore all others), then memory_retrieve_text (at most 2 calls) "
    "before working. Do not touch any other context."
)
ARM_NOTES_S2 = (
    "ARM BLOCK (self-notes): A previous session worked on this same task and may have left NOTES.md in the work "
    "directory. Read it first if present."
)

S1_BODY = (
    "# {name} - session 1 (discover)\n\n"
    "Work strictly inside this directory: the run work dir.\n"
    "{goal}\n"
    "Save your final findings in your reply; the runner saves it as report-s1.md.\n\n"
)

S2_BODY = (
    "# {name} - session 2 (use the previous session's findings)\n\n"
    "Work strictly inside this directory: the run work dir.\n"
    "You are a NEW session. A previous session already worked on this task and its findings may exist in this work dir.\n"
    "{goal}\n"
    "The runner saves your reply as report-s2.md. Do not redo discovery you can trust.\n\n"
)

TASKS = [
 dict(id="t1", name="NightlySync", ctx="b12-t1-cmhm",
  files={
   "sched/NightSync.xml": (
    '<?xml version="1.0"?>\n<Task version="1.2">'
    '<Triggers><TimeTrigger><StartBoundary>2026-03-01T03:00:00</StartBoundary></TimeTrigger></Triggers>'
    '<Actions><Exec><Command>C:\\ops\\bin\\sync.ps1</Command><WorkingDirectory>C:\\ops</WorkingDirectory></Exec></Actions>'
    '<Principals><UserId>SVC-SYNC</UserId></Principals></Task>\n'),
   "scripts/sync-nightly.ps1": (
    '$conf = Read-Config "$PSScriptRoot\\..\\conf\\sync.toml"\n'
    'if (-not $conf.sync.enabled) { exit 0 }   # silent no-op when disabled\n'
    'Start-Sync $conf.targets\n'
    'Write-AppLog "$PSScriptRoot\\..\\logs\\sync.log" "completed run=$conf.run_id"\n'),
   "conf/sync.toml": (
    "[sync]\nenabled = false     # flipped off during the March freeze, never restored\n"
    'run_id = "night-a"\ntargets = ["e:/vault", "e:/backup"]\n'),
   "logs/sync.log": (
    "2026-05-02 03:04:11 INFO completed run=night-a\n2026-04-30 03:04:02 INFO completed run=night-a\n"
    "2026-04-28 02:58:59 INFO completed run=night-b\n"),
  },
  s1=("The NightlySync task has silently stopped running. Diagnose the COMPLETE reason chain in this directory. "
      "Report exactly: (1) the task schedule file and the command path it lists, (2) which of those paths actually exist in this tree, "
      "(3) the config file and the flag that gates the script, with its current value, (4) the date of the last successful recorded run."),
  s2=("Apply the minimal complete set of edits so the task would start running again, WITHOUT deleting anything "
      "(no file removals, no reformatting of untouched files). Then write ops/fix-log.txt with exactly three lines: "
      "the schedule file name, the corrected script path it must point to, and the flag with the value you set."),
 ),
 dict(id="t2", name="IniOverride", ctx="b12-t2-cmhm",
  files={
   "app/app.ini": "[server]\nmode = PROD\n\ninclude=../env/overrides.ini\n",
   "env/overrides.ini": "[server]\nmode = STAGING\nmaxconn = 200\nregion = eu\n",
   "app/load.py": (
    "import configparser, os\n\n"
    "def load(path):\n"
    "    p = configparser.ConfigParser(); p.read(path)\n"
    "    for incl in [s.strip() for s in p.get('server','include',fallback='').split(',') if s.strip()]:\n"
    "        q = configparser.ConfigParser(); q.read(os.path.join(os.path.dirname(path), incl))\n"
    "        for sec in q.sections():\n"
    "            for k, v in q.items(sec): p.set(sec, k, v)   # includes ALWAYS win\n"
    "    return p\n"),
   "docs/README.txt": ("The loader applies the main ini first, then every include listed under 'include=', in order.\n"),
  },
  s1=("The deployed service reports mode STAGING even though app/app.ini says PROD. Explain the exact mechanism "
      "(include handling in app/load.py) and report: the include chain, which file wins, and which keys come from the winning file."),
  s2=("Make prod runs resolve mode = PROD without deleting or editing app/app.ini or env/overrides.ini. "
      "Create env/prod.ini (carrying mode = PROD and maxconn = 250) and change ONLY app/load.py so that when '--env prod' "
      "is passed on its command line, prod.ini is applied after any includes. The default behavior (no flag) keeps the same output."),
 ),
 dict(id="t3", name="CalFlags", ctx="b12-t3-cmhm",
  files={
   "tools/calctl.py": "FLAGS = {\n    'SHADOW': 1,\n    'TINT':   2,\n    'GRID':   4,\n    'FULL':   8,\n}\n",
   "docs/README.md": "# calctl flags\n- SHADOW = 1\n- TINT = 2\n- GRID = 6\n- FULL = 8\n",
  },
  s1="Cross-check tools/calctl.py against docs/README.md and report every doc/code mismatch with the code's actual value.",
  s2=("Correct docs/README.md so every flag matches the code value exactly (docs follow code, never the reverse; "
      "do not touch tools/calctl.py). Then create tools/changelog.txt whose first line is exactly: 'flags doc synced to calctl.py'."),
 ),
 dict(id="t4", name="MakeShadow", ctx="b12-t4-cmhm",
  files={
   "Makefile": (
    ".PHONY: build\nbuild:\n\t@echo building $$(VAR)\n\t@mkdir -p $$(VAR) && cp src/main.c $$(VAR)/\n\ninclude config/defaults.mk\n"),
   "config/defaults.mk": "VAR = build/dev\nBUILD_LAYERS = 2\n",
   "src/main.c": "/* tiny fixture file */\nint main(void){return 0;}\n",
  },
  s1=("The build lands in the wrong directory and echoes the wrong value for VAR. Diagnose why the assignment "
      "in config/defaults.mk has no effect on the build rule, and report the include-placement fact."),
  s2=("Fix Makefile so config/defaults.mk takes effect (edit ONLY Makefile; do not touch defaults.mk or main.c). "
      "Then create build/build-note.txt with one line explaining the cause (must mention precedence or placement)."),
 ),
 dict(id="t5", name="EnvOrder", ctx="b12-t5-cmhm",
  files={
   "env/common.env": ("DB_URL=postgres://prod-db.local:5432/app\nMAIL_RELAY=smtp-prod.local\n"),
   "env/local.env": "DB_URL=postgres://dev-db.local:5432/appdev\n",
   "scripts/loadenv.ps1": ("param()\n$here = $PSScriptRoot\n. \"$here\\..\\env\\common.env\"\n. \"$here\\..\\env\\local.env\"\n"),
   "src/run_app.ps1": ("param([string]$EnvName)\nif ($EnvName -eq 'dev') { throw 'dev requires the local DB override first' }\nWrite-Host \"using $env:DB_URL\"\n"),
  },
  s1=("A dev run is attempting to use the PRODUCTION database URL. Find where the environment is assembled, "
      "in which order the two env files are sourced, and why the dev value cannot win in the current order."),
  s2=("Change ONLY scripts/loadenv.ps1 so env/local.env is sourced BEFORE env/common.env; leave everything else untouched. "
      "Create dev/env-fix.txt whose single line names the two files in the NEW sourcing order."),
 ),
 dict(id="t6", name="MigGap", ctx="b12-t6-cmhm",
  files={
   "db/migrations/001_init.sql": "CREATE TABLE t (id INTEGER);\n",
   "db/migrations/002_add_days.sql": "ALTER TABLE t ADD day TEXT;\n",
   "db/migrations/004_add_tag.sql": "ALTER TABLE t ADD tag TEXT;\n",
   "db/migrate.ps1": ("# applies any migration whose number is NOT already in the marker table\n"
                      "# aborts immediately if the numbering has a GAP\n"
                      "# numbers must be consecutive from 001\n"),
   "db/schema_migrations.csv": "001,applied\n002,applied\n004,applied\n",
  },
  s1=("db/migrate.ps1 aborts on a broken numbering scheme. Report: which number is missing, which tool already treats "
      "a higher number as applied, and why the abort is deliberate (which behavior the script documents)."),
  s2=("Restore consecutive numbering WITHOUT changing migrate.ps1, without touching 001/002/004, and without altering "
      "the applied markers: create db/migrations/003_noop_backfill.sql containing only the line "
      "'-- no-op backfill (recovered gap)'. Then create db/mig-note.txt stating why 003 must carry no schema change."),
 ),
 dict(id="t7", name="RouteOrder", ctx="b12-t7-cmhm",
  files={
   "routes/app.routes": ("/blog/*        -> blog.handler\n/static/*      -> assets.handler\n/health        -> health.handler\n"),
   "docs/routing.txt": "First matching entry (top to bottom) serves every request.\n",
   "logs/access.log": ("GET /static/app.js -> served by blog.handler\nGET /static/app.css -> served by blog.handler\nGET /blog/a -> served by blog.handler\n"),
  },
  s1=("Some /static requests are served by the blog handler. Explain the routing fact that causes this and "
      "report the access-log lines that prove it."),
  s2=("Edit routes/app.routes so /static/* and /health can no longer be shadowed by /blog/* (order them ahead of it; "
      "do not rename handlers, do not touch docs/). Then create routes/reorder-note.txt with one line naming the "
      "shadowed path and the winning path."),
 ),
 dict(id="t8", name="Entrypoint", ctx="b12-t8-cmhm",
  files={
   "pkg/pyproject.toml": "[project.scripts]\ndatajob = \"pkg.cli:main\"\n",
   "docs/README.txt": "Run jobs with: data-jobs --all\n",
   "pkg/repro.txt": ("$ data-jobs --all\ndata-jobs: command not found\n$ datajob --all\n[main] running with --all\n"),
  },
  s1="Report why `data-jobs` fails while `datajob` works, citing the pyproject entry that decides command names.",
  s2=("Align the project so the documented command works: edit pkg/pyproject.toml to expose BOTH names "
      "(data-jobs and datajob must both appear). Do not touch docs. Create pkg/rename-note.txt listing both names."),
 ),
 dict(id="t9", name="LogHunt", ctx="b12-t9-cmhm",
  files={
   "logs/app-1.log": ("02:14 WARN slow query 31ms\n05:40 WARN retry after bloom\n11:02 WARN pool warmup\n"),
   "logs/app-2.log": ("03:01 WARN de-dupe before finalize\n09:59 WARN swapper mb\n13:20 WARN stale index\n15:44 WARN latency budget 11/10ms\n"),
   "logs/app-3.log": ("04:06 WARN single retry\n08:15 WARN slow exec 55ms\n21:37 ERROR E-PAY-1183 ledger rejected settlement record\n23:58 WARN cool down\n"),
   "logs/app-4.log": ("01:15 WARN cache nudged\n07:31 WARN quiesce window\n18:02 WARN quickly retried noop\n"),
   "logs/app-5.log": ("02:48 WARN qlen 3\n10:10 WARN ephemeral port reuse\n19:05 WARN old ticker stopped\n"),
  },
  s1=("Hunt the logs directory for a real failure. All WARN lines are noise; exactly one ERROR defines an actual fault. "
      "Report its error code, the file that contains it, and the exact failing timestamp."),
  s2=("Write logs/summary.txt with exactly three lines: the error code, the file that contains it, and the exact timestamp "
      "of the failing record. Your report reply must quote the file name and the code."),
 ),
 dict(id="t10", name="PinDrift", ctx="b12-t10-cmhm",
  files={
   "req/pin.lock": "notice-utils==1.2.3\nserialization-ffi==0.9.0\n",
   "req/requirements.txt": "notice-utils==1.2.3\nserialization-ffi>=0.9,<0.10\n",
   "scripts/install.sh": ("#!/usr/bin/env bash\nset -euo pipefail\nexport PIP_INDEX_URL=https://mirror-a.local/pypi/simple\n"
                          "pip install -r req/requirements.txt\n"),
   "docs/audit.txt": "2026-10-02 installed notice-utils 1.2.4 from mirror-a (pinned 1.2.3 - drift!)\n",
  },
  s1=("A pinned dependency was installed at a different version. Report: the pinned version, the version the audit found "
      "installed, and where the decision is made (which script and which variable)."),
  s2=("Create req/drift.txt with exactly three lines: package name, expected version, installed version. Then edit "
      "scripts/install.sh's pip line so it also pins the exact expected version, keeping the mirror URL untouched and "
      "not changing any other file."),
 ),
]

def write(p: pathlib.Path, txt: str):
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(txt, encoding="utf-8", newline="\n")

def materialize():
    for t in TASKS:
        base = ROOT / "tasks" / f"b12-{t['id']}"
        md = (
            f"# B12 task {t['id']} - {t['name']}\n\n"
            f"Arms share these bodies verbatim; ONLY the arm block differs (README/tools eval_b12).\n"
            f"cmhm arms use context display_name prefix '{t['ctx']}'.\n\n"
            "## SESSION 1 PROMPT (body used verbatim for every arm)\n\n"
            + S1_BODY.format(name=t['name'], goal=t['s1'])
            + "## SESSION-1 ARM BLOCKS (append verbatim to the body)\n\n"
            + f"- none:       {ARM_NONE_S1}\n"
            + f"- self-notes: {ARM_NOTES_S1}\n"
            + f"- cmhm:       {ARM_CMHM_S1.format(ctx=t['ctx'])}\n\n"
            "## SESSION 2 PROMPT (body used verbatim for every arm)\n\n"
            + S2_BODY.format(name=t['name'], goal=t['s2'])
            + "## SESSION-2 ARM BLOCKS (append verbatim to the body)\n\n"
            + "- none:       (no block - body only)\n"
            + f"- self-notes: {ARM_NOTES_S2}\n"
            + f"- cmhm:       {ARM_CMHM_S2.format(ctx=t['ctx'])}\n\n"
            "## VERIFIER\n\n"
            "Rubric pre-registered in tools/eval_b12/verify.py under this task id.\n\n"
        )
        write(base / "task.md", md)
        for rel, content in t['files'].items():
            write(base / 'fixtures' / rel, content)
    print("materialized", len(TASKS), "tasks under tools/eval_b12/tasks/")

if __name__ == "__main__":
    materialize()
