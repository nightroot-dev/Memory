#!/usr/bin/env python3
"""Mine external git history into (query, trigger, hard-negative) triplets.

Phase 5, Slice 3. The corpus is built ONLY from cloned repositories under
``tools/phase5_data/repos``. This script never reads CMHM evaluation corpora:
the output path and every clone path are checked against the quarantine list
before any filesystem work.

Each triplet is:

    query     -- the commit's issue/symptom text (what a user would ask)
    positive  -- a normalized trigger signature built from the fix diff:
                 ``[domain | module | exact symbols] symptom :: removed code``
    negative  -- a different fix's trigger signature from the same file
                 (fallback: same directory), i.e. a hard negative

Outputs:
    triplets.jsonl       all mined rows
    train.jsonl          training split
    val.jsonl            validation split (held-out repos by default)
    test.jsonl           test split (held-out repos by default)
    audit_corpus.rs      query-only Rust corpus for scripts/audit_corpus_overlap.ps1
    manifest.json        counts, repos, commits, and audit status

Usage:
    python tools/mine_git_triplets.py
    python tools/mine_git_triplets.py --repos tokio-rs/tokio,axios/axios
    python tools/mine_git_triplets.py --skip-clone   # reuse existing clones
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUT = REPO_ROOT / "tools" / "phase5_data"
DEFAULT_CLONE_ROOT = DEFAULT_OUT / "repos"

# Hard contamination firewall. These paths are never read, copied, or written.
FORBIDDEN_DIR_NAMES = {
    "eval_b8",
    "eval_b9",
    "eval_b10",
    "eval_b11",
    "eval_b12",
    "eval_b13",
    "eval_d2",
    "eval_l3",
    "eval_w1",
}
FORBIDDEN_FILE_NAMES = {"corpus_l1.json", "validated_pairs.json"}


def _resolve(path: Path) -> Path:
    try:
        return path.resolve()
    except OSError:
        return path.absolute()


def assert_not_forbidden(path: Path) -> None:
    """Fail closed if a path is inside a quarantined eval directory/file."""
    p = _resolve(path)
    for part in p.parts:
        if part in FORBIDDEN_DIR_NAMES:
            raise SystemExit(f"refusing quarantined path: {p}")
    if p.name in FORBIDDEN_FILE_NAMES:
        raise SystemExit(f"refusing quarantined file: {p}")


DEFAULT_REPOS: list[tuple[str, str]] = [
    ("tokio-rs/tokio", "rust"),
    ("serde-rs/serde", "rust"),
    ("clap-rs/clap", "rust"),
    ("BurntSushi/ripgrep", "rust"),
    ("sharkdp/bat", "rust"),
    ("vitejs/vite", "typescript"),
    ("axios/axios", "typescript"),
    ("nestjs/nest", "typescript"),
    ("sveltejs/svelte", "typescript"),
    ("trpc/trpc", "typescript"),
]

SOURCE_EXTS = {".rs", ".ts", ".tsx", ".js", ".jsx"}
FIX_RE = re.compile(r"\b(fix|fixes|fixed|bugfix|hotfix|revert)\b", re.I)
GENERIC_SUBJECT_RE = re.compile(
    r"^(fix|fixes|fixed|bugfix|hotfix|revert)(\([^)]*\))?!?\s*:?\s*$", re.I
)
PREFIX_RE = re.compile(
    r"^(fix|fixes|fixed|bugfix|hotfix|revert)(\([^)]*\))?!?\s*:\s*", re.I
)
TRAILER_RE = re.compile(
    r"(?im)^(signed-off-by|co-authored-by|reviewed-by|change-id|refs?|see also)\s*:.*$"
)
URL_RE = re.compile(r"https?://\S+")
ISSUE_RE = re.compile(r"(?<!\w)#\d+\b")
IDENT_RE = re.compile(r"\b[A-Za-z_][A-Za-z0-9_]{2,}\b")

STOP_IDENTS = {
    "self", "this", "that", "true", "false", "null", "none", "some", "ok", "err",
    "return", "function", "const", "let", "mut", "pub", "fn", "impl", "use", "mod",
    "import", "export", "from", "async", "await", "class", "interface", "type",
    "enum", "struct", "match", "else", "for", "while", "loop", "break", "continue",
    "new", "string", "number", "boolean", "vec", "option", "result", "unwrap",
    "expect", "println", "eprintln", "format", "into", "iter", "map", "filter",
    "collect", "clone", "default", "derive", "debug", "display", "error", "warn",
    "info", "trace", "the", "and", "with", "when", "where", "what", "which", "have",
    "should", "would", "could", "there", "their", "about", "after", "before",
    "because", "while", "during", "under", "over", "into", "than", "then", "them",
    "they", "this", "these", "those", "from", "some", "such", "only", "also",
    "does", "doesn", "didn", "isn", "wasn", "weren", "can", "cannot", "not",
    "get", "set", "add", "remove", "update", "create", "delete", "test", "tests",
}

DOMAIN_BY_LANG = {"rust": "rust", "typescript": "typescript"}


def run(cmd: list[str], cwd: Path | None = None, timeout: int = 300) -> str:
    proc = subprocess.run(
        cmd,
        cwd=str(cwd) if cwd else None,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=timeout,
    )
    if proc.returncode != 0:
        raise RuntimeError(
            f"command failed ({proc.returncode}): {' '.join(cmd)}\n{proc.stderr[-2000:]}"
        )
    return proc.stdout


def clean_text(text: str) -> str:
    text = TRAILER_RE.sub(" ", text)
    text = URL_RE.sub(" ", text)
    text = ISSUE_RE.sub(" ", text)
    text = re.sub(r"\s+", " ", text)
    return text.strip()


def first_sentence(text: str, limit: int = 280) -> str:
    text = clean_text(text)
    if not text:
        return ""
    parts = re.split(r"(?<=[.!?])\s+", text, maxsplit=1)
    sentence = parts[0] if parts else text
    return sentence[:limit].strip()


def clean_subject(subject: str) -> str:
    subject = PREFIX_RE.sub("", subject.strip())
    return clean_text(subject)


def query_from_commit(subject: str, body: str) -> str:
    subject_clean = clean_subject(subject)
    body_clean = clean_text(body)
    if GENERIC_SUBJECT_RE.match(subject_clean) or len(subject_clean) < 18:
        symptom = first_sentence(body_clean)
        if symptom:
            return symptom[:700]
    if body_clean:
        return f"{subject_clean}. {body_clean}"[:700].strip()
    return subject_clean[:700]


def sanitize_component(text: str, limit: int) -> str:
    text = re.sub(r"[\[\]|]", " ", text)
    text = text.replace('"', "'")
    text = re.sub(r"\s+", " ", text).strip()
    return text[:limit].strip()


def module_from_path(path: str, lang: str) -> str:
    p = path.replace("\\", "/")
    for prefix in ("src/", "lib/", "crates/", "packages/"):
        if p.startswith(prefix):
            p = p[len(prefix):]
            break
    p = re.sub(r"\.(rs|ts|tsx|js|jsx)$", "", p, flags=re.I)
    for suffix in ("/mod", "/index"):
        if p.endswith(suffix):
            p = p[: -len(suffix)]
    sep = "::" if lang == "rust" else "/"
    return p.replace("/", sep)[:80] or "root"


def symbols_from_lines(lines: list[str], limit: int = 6) -> list[str]:
    counts: dict[str, int] = {}
    for line in lines:
        for token in IDENT_RE.findall(line):
            if token.lower() in STOP_IDENTS or len(token) < 3:
                continue
            if token.isdigit():
                continue
            counts[token] = counts.get(token, 0) + 1
    ordered = sorted(counts.items(), key=lambda kv: (-kv[1], -len(kv[0]), kv[0]))
    return [token for token, _ in ordered[:limit]]


def parse_diff(diff_text: str) -> dict[str, dict[str, list[str]]]:
    """Return {path: {"removed": [...], "added": [...]}} for source files."""
    result: dict[str, dict[str, list[str]]] = {}
    current: str | None = None
    for line in diff_text.splitlines():
        if line.startswith("diff --git "):
            current = None
            m = re.match(r"diff --git a/(.+?) b/(.+)$", line)
            if m:
                current = m.group(2)
                if Path(current).suffix.lower() in SOURCE_EXTS:
                    result.setdefault(current, {"removed": [], "added": []})
                else:
                    current = None
            continue
        if current is None:
            continue
        if line.startswith("---") or line.startswith("+++"):
            continue
        if line.startswith("-"):
            body = line[1:].strip()
            if body and not body.startswith(("//", "/*", "*")):
                result[current]["removed"].append(body)
        elif line.startswith("+"):
            body = line[1:].strip()
            if body and not body.startswith(("//", "/*", "*")):
                result[current]["added"].append(body)
    return result


def is_test_path(path: str) -> bool:
    p = path.replace("\\", "/").lower()
    return (
        "/tests/" in p
        or p.startswith("tests/")
        or "/__tests__/" in p
        or p.endswith("_test.rs")
        or ".test." in p
        or ".spec." in p
    )


def commit_records(repo: Path, max_commits: int) -> list[dict]:
    marker = "@@CMHM_COMMIT_7f3a@@"
    out = run(
        [
            "git",
            "log",
            "--no-merges",
            "--date=iso-strict",
            f"--pretty=format:{marker}%H%x1f%ad%x1f%s%x1f%b",
            "--name-only",
        ],
        cwd=repo,
        timeout=600,
    )
    records: list[dict] = []
    for raw in out.split(marker):
        raw = raw.strip("\r\n")
        if not raw:
            continue
        # The body may contain blank lines; the file list is always the last
        # blank-line-separated block in the record.
        parts = raw.split("\n\n")
        files_blob = parts[-1] if len(parts) >= 2 else ""
        header = "\n\n".join(parts[:-1]) if len(parts) >= 2 else raw
        fields = header.split("\x1f", 3)
        if len(fields) < 4:
            continue
        commit, date, subject, body = fields[0], fields[1], fields[2], fields[3]
        message = f"{subject} {body}"
        if not FIX_RE.search(message):
            continue
        files = [
            line.strip()
            for line in files_blob.splitlines()
            if line.strip() and Path(line.strip()).suffix.lower() in SOURCE_EXTS
        ]
        if not files:
            continue
        files = [f for f in files if not is_test_path(f)]
        if not files:
            continue
        records.append(
            {
                "commit": commit,
                "date": date,
                "subject": subject,
                "body": body,
                "files": files[:8],
            }
        )
        if len(records) >= max_commits:
            break
    return records


def mine_repo(repo_slug: str, lang: str, clone_root: Path, max_commits: int) -> list[dict]:
    dest = clone_root / repo_slug.replace("/", "__")
    assert_not_forbidden(dest)
    if not dest.exists():
        print(f"  cloning {repo_slug} ...", flush=True)
        dest.parent.mkdir(parents=True, exist_ok=True)
        run(
            ["git", "clone", "--no-checkout", "--single-branch", f"https://github.com/{repo_slug}.git", str(dest)],
            timeout=1800,
        )
    else:
        print(f"  reusing clone {repo_slug}", flush=True)

    records = commit_records(dest, max_commits)
    print(f"  {repo_slug}: {len(records)} fix-shaped commits", flush=True)

    entries: list[dict] = []
    for idx, rec in enumerate(records):
        if idx and idx % 100 == 0:
            print(f"    {idx}/{len(records)} commits, {len(entries)} triplets", flush=True)
        query = query_from_commit(rec["subject"], rec["body"])
        if len(query) < 30:
            continue
        try:
            diff = run(
                [
                    "git",
                    "show",
                    "--format=",
                    "--no-color",
                    "--no-ext-diff",
                    "--unified=0",
                    rec["commit"],
                    "--",
                    *rec["files"],
                ],
                cwd=dest,
                timeout=120,
            )
        except (RuntimeError, subprocess.TimeoutExpired):
            continue
        per_file = parse_diff(diff)
        symptom = first_sentence(rec["body"]) or clean_subject(rec["subject"])
        if len(symptom) < 12:
            symptom = clean_subject(rec["subject"])[:200]
        for path, hunks in per_file.items():
            removed = hunks["removed"][:3]
            added = hunks["added"][:2]
            code_lines = removed or added
            if not code_lines:
                continue
            symbols = symbols_from_lines(code_lines)
            if not symbols:
                continue
            module = module_from_path(path, lang)
            code = sanitize_component(" ".join(code_lines), 220)
            trigger = (
                f"[{DOMAIN_BY_LANG[lang]} | {module} | {', '.join(symbols)}] "
                f"{sanitize_component(symptom, 180)} :: {code}"
            )[:512]
            entries.append(
                {
                    "query": query,
                    "positive": trigger,
                    "repo": repo_slug,
                    "lang": lang,
                    "commit": rec["commit"],
                    "date": rec["date"],
                    "file": path,
                    "is_revert": bool(re.search(r"\brevert\b", rec["subject"], re.I)),
                }
            )
    return entries


def attach_negatives(entries: list[dict]) -> list[dict]:
    by_file: dict[tuple[str, str], list[dict]] = {}
    by_dir: dict[tuple[str, str], list[dict]] = {}
    for entry in entries:
        by_file.setdefault((entry["repo"], entry["file"]), []).append(entry)
        parent = str(Path(entry["file"]).parent).replace("\\", "/")
        by_dir.setdefault((entry["repo"], parent), []).append(entry)

    kept: list[dict] = []
    seen_queries: set[str] = set()
    for entry in entries:
        qnorm = re.sub(r"\s+", " ", entry["query"].lower()).strip()
        if qnorm in seen_queries:
            continue
        candidates = [
            e
            for e in by_file.get((entry["repo"], entry["file"]), [])
            if e["commit"] != entry["commit"] and e["positive"] != entry["positive"]
        ]
        kind = "same_file"
        if not candidates:
            parent = str(Path(entry["file"]).parent).replace("\\", "/")
            candidates = [
                e
                for e in by_dir.get((entry["repo"], parent), [])
                if e["commit"] != entry["commit"] and e["positive"] != entry["positive"]
            ]
            kind = "same_dir"
        if not candidates:
            continue
        digest = hashlib.sha1(f"{entry['commit']}:{entry['file']}".encode()).digest()
        neg = candidates[int.from_bytes(digest[:4], "big") % len(candidates)]
        row = dict(entry)
        row["negative"] = neg["positive"]
        row["negative_kind"] = kind
        row["negative_commit"] = neg["commit"]
        kept.append(row)
        seen_queries.add(qnorm)
    return kept


def write_jsonl(path: Path, rows: list[dict]) -> None:
    assert_not_forbidden(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="\n") as fh:
        for row in rows:
            fh.write(json.dumps(row, ensure_ascii=False) + "\n")


def rust_literal(text: str) -> str:
    clean = (
        text.replace("\\", "/")
        .replace('"', "'")
        .replace("\r", " ")
        .replace("\n", " ")
    )
    clean = "".join(ch if 32 <= ord(ch) < 127 else " " for ch in clean)
    clean = re.sub(r"\s+", " ", clean).strip()
    return f'"{clean}"'


def write_audit_corpus(path: Path, rows: list[dict]) -> None:
    assert_not_forbidden(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [
        "// Phase 5 mined-corpus overlap audit input.",
        "// Generated by tools/mine_git_triplets.py; query-only by design.",
        "",
        "pub struct Case {",
        "    pub query: &'static str,",
        "}",
        "",
        "pub const CASES: &[Case] = &[",
    ]
    for row in rows:
        lines.append(f"    Case {{ query: {rust_literal(row['query'])} }},")
    lines.append("];")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")


def split_rows(
    rows: list[dict], val_repos: set[str], test_repos: set[str]
) -> tuple[list[dict], list[dict], list[dict]]:
    train: list[dict] = []
    val: list[dict] = []
    test: list[dict] = []
    for row in rows:
        if row["repo"] in test_repos:
            test.append(row)
        elif row["repo"] in val_repos:
            val.append(row)
        else:
            train.append(row)
    if not val and rows:
        val = rows[: max(1, len(rows) // 10)]
    if not test and len(rows) > len(val):
        test = rows[len(val): len(val) + max(1, len(rows) // 10)]
    return train, val, test


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", default=str(DEFAULT_OUT))
    parser.add_argument("--clone-root", default=str(DEFAULT_CLONE_ROOT))
    parser.add_argument(
        "--repos",
        default="",
        help="comma-separated owner/name list; default is the built-in curated set",
    )
    parser.add_argument("--max-commits-per-repo", type=int, default=900)
    parser.add_argument("--val-repos", default="vitejs/vite")
    parser.add_argument("--test-repos", default="tokio-rs/tokio")
    parser.add_argument("--skip-clone", action="store_true", help="reuse existing clones only")
    args = parser.parse_args()

    out = Path(args.out)
    clone_root = Path(args.clone_root)
    assert_not_forbidden(out)
    assert_not_forbidden(clone_root)
    out.mkdir(parents=True, exist_ok=True)
    clone_root.mkdir(parents=True, exist_ok=True)

    if args.repos:
        wanted = [r.strip() for r in args.repos.split(",") if r.strip()]
        lang_by_repo = dict(DEFAULT_REPOS)
        repos = [(r, lang_by_repo.get(r, "rust")) for r in wanted]
    else:
        repos = DEFAULT_REPOS

    if args.skip_clone:
        missing = [
            r for r, _ in repos if not (clone_root / r.replace("/", "__")).exists()
        ]
        if missing:
            print(f"missing clones with --skip-clone: {', '.join(missing)}", file=sys.stderr)
            return 2

    all_entries: list[dict] = []
    started = time.time()
    for repo_slug, lang in repos:
        print(f"[{repo_slug}]", flush=True)
        try:
            all_entries.extend(mine_repo(repo_slug, lang, clone_root, args.max_commits_per_repo))
        except (RuntimeError, subprocess.TimeoutExpired) as exc:
            print(f"  skipped {repo_slug}: {exc}", file=sys.stderr, flush=True)

    rows = attach_negatives(all_entries)
    if len(rows) < 40:
        print(f"only {len(rows)} triplets mined; need at least 40", file=sys.stderr)
        return 1

    val_repos = {r.strip() for r in args.val_repos.split(",") if r.strip()}
    test_repos = {r.strip() for r in args.test_repos.split(",") if r.strip()}
    train, val, test = split_rows(rows, val_repos, test_repos)

    write_jsonl(out / "triplets.jsonl", rows)
    write_jsonl(out / "train.jsonl", train)
    write_jsonl(out / "val.jsonl", val)
    write_jsonl(out / "test.jsonl", test)
    write_audit_corpus(out / "audit_corpus.rs", rows)

    manifest = {
        "generator": "tools/mine_git_triplets.py",
        "version": 1,
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "elapsed_sec": round(time.time() - started, 1),
        "repos": [r for r, _ in repos],
        "val_repos": sorted(val_repos),
        "test_repos": sorted(test_repos),
        "counts": {
            "all": len(rows),
            "train": len(train),
            "val": len(val),
            "test": len(test),
        },
        "commits": sorted({row["commit"] for row in rows}),
        "audit": {
            "script": "scripts/audit_corpus_overlap.ps1",
            "candidate": "tools/phase5_data/audit_corpus.rs",
            "status": "pending",
        },
    }
    (out / "manifest.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n"
    )
    print(
        f"wrote {len(rows)} triplets "
        f"(train={len(train)}, val={len(val)}, test={len(test)}) to {out}",
        flush=True,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
