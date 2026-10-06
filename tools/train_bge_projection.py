#!/usr/bin/env python3
"""
Train a 384 -> 128 projection matrix for MiniLM embeddings.

Backend: fastembed (ONNX, no torch) — this is the same backend the Rust
engine uses when compiled with --features onnx, so the projection is
trained on the exact embedding distribution the engine will produce.

Approach: PCA on a diverse corpus of MiniLM (all-MiniLM-L6-v2) embeddings.
Top 128 principal components form the projection.

Output format (matches cmhm_core::projection::Projection::from_bytes):
    [input_dim: u32 LE = 384]
    [matrix: 128 * 384 f32 LE, row-major]
    [bias: 128 f32 LE]

Rust projection semantics:
    out[row] = bias[row] + sum_col matrix[row, col] * e[col]
             = W.T @ e + bias

PCA decomposition (W: 384x128 components, mu: 384 mean):
    y = W.T @ (e - mu) = W.T @ e - W.T @ mu
    => matrix = W.T          (128 x 384)
    => bias   = -W.T @ mu    (128)
"""

import sys
import numpy as np
from pathlib import Path

try:
    from fastembed import TextEmbedding
except ImportError:
    print("[D] FATAL: fastembed not installed. Run: pip install fastembed", file=sys.stderr)
    sys.exit(1)

MODEL_NAME = "BAAI/bge-base-en-v1.5"
INPUT_DIM = 768
OUTPUT_DIM = 128
REPO_ROOT = Path(__file__).resolve().parent.parent
OUT_PATH = REPO_ROOT / "data" / "projections" / "bge-base-en-v1.5.bin"


def collect_repo_lines() -> list[str]:
    lines: list[str] = []
    patterns = ["**/*.rs", "**/*.py", "**/*.md", "**/*.toml"]
    for pat in patterns:
        for path in REPO_ROOT.glob(pat):
            s = str(path)
            if ".git" in s or "target/" in s or "node_modules" in s:
                continue
            try:
                text = path.read_text(encoding="utf-8", errors="ignore")
            except Exception:
                continue
            for raw in text.split("\n"):
                line = raw.strip().strip("/*#").strip()
                if 20 <= len(line) <= 250 and " " in line:
                    lines.append(line)
    return lines


def collect_synthetic() -> list[str]:
    subjects = [
        "database", "api service", "authentication layer", "billing system",
        "cache", "message queue", "load balancer", "reverse proxy",
        "kubernetes pod", "container registry", "backup job", "migration",
        "scheduler", "worker pool", "search index", "log aggregator",
        "metrics pipeline", "session store", "rate limiter", "config loader",
    ]
    failures = [
        "failed to start", "timed out", "crashed unexpectedly",
        "returned error 500", "rejected the request", "exhausted connection pool",
        "hit memory limit", "leaked file descriptors", "deadlocked",
        "returned 429 rate limit",
    ]
    successes = [
        "restarted cleanly", "recovered after retry", "processed the batch",
        "served the request in 20ms", "completed the migration",
        "synced without errors", "cleared the queue", "warmed the cache",
        "published the event", "acknowledged the message",
    ]
    actions = [
        "restarting", "rolling back", "scaling up", "increasing timeout",
        "clearing the cache", "rotating credentials", "bumping replicas",
        "enabling debug logging", "draining the node", "applying the patch",
    ]
    out: list[str] = []
    for s in subjects:
        for f in failures:
            out.append(f"the {s} {f} during peak traffic")
            out.append(f"diagnosed that the {s} {f} after deployment")
        for ok in successes:
            out.append(f"the {s} {ok} after the fix")
            out.append(f"observed that the {s} {ok} on the second attempt")
        for a in actions:
            out.append(f"{a} the {s} resolved the incident")
            out.append(f"the situation required {a} the {s}")
    return out


def collect_generic() -> list[str]:
    base = [
        "The user reported a problem with the billing page.",
        "We need to investigate why the deployment failed overnight.",
        "After restarting the service, the error stopped appearing.",
        "The database returned unexpected results for this query.",
        "A user cannot log in after resetting their password.",
        "The API responded with a 500 error under load.",
        "Latency increased noticeably during peak hours yesterday.",
        "The scheduled backup did not complete as expected.",
        "We should check the logs before making any changes.",
        "The configuration change broke the integration tests.",
        "Customer satisfaction improved after the last release.",
        "The monitoring alert fired three times this week.",
        "Memory usage grows steadily until the process restarts.",
        "The feature was rolled back due to a regression.",
        "Documentation needs updating for the new endpoint.",
        "This workaround is temporary and should be replaced.",
        "The team agreed to postpone the migration to next quarter.",
        "Performance improved after enabling query caching.",
        "Access logs show an unusual spike in traffic.",
        "The third-party service outage affected our users.",
    ]
    # Deterministic expansion — 10 variants per base sentence
    prefixes = [
        "", "Yesterday, ", "On Monday, ", "During the incident, ",
        "According to the report, ", "In the postmortem, ",
        "The team noted that ", "We observed that ",
        "An engineer mentioned that ", "The dashboard showed that ",
    ]
    out: list[str] = []
    for p in prefixes:
        for b in base:
            out.append(f"{p}{b[0].lower()}{b[1:]}" if p else b)
    return out


def build_corpus() -> list[str]:
    repo = collect_repo_lines()
    synth = collect_synthetic()
    generic = collect_generic()
    seen = set()
    corpus: list[str] = []
    for text in repo + synth + generic:
        if text in seen:
            continue
        seen.add(text)
        corpus.append(text)
    return corpus


def embed_corpus(corpus: list[str]) -> np.ndarray:
    print(f"[D] Loading fastembed model: {MODEL_NAME}")
    model = TextEmbedding(model_name=MODEL_NAME)

    print(f"[D] Embedding {len(corpus)} texts (batched)...")
    # fastembed.embed returns a generator of np.ndarray[float32]
    vecs = []
    batch_size = 64
    for i in range(0, len(corpus), batch_size):
        chunk = corpus[i : i + batch_size]
        for v in model.embed(chunk):
            vecs.append(np.asarray(v, dtype=np.float32))
        if (i // batch_size) % 20 == 0:
            print(f"[D]   {i + len(chunk)}/{len(corpus)}")

    arr = np.stack(vecs, axis=0)
    assert arr.shape[1] == INPUT_DIM, f"expected {INPUT_DIM} dims, got {arr.shape[1]}"

    # Re-normalize defensively — some fastembed versions already L2-normalize,
    # some do not. Idempotent if already unit norm.
    norms = np.linalg.norm(arr, axis=1, keepdims=True)
    norms[norms < 1e-12] = 1.0
    arr = arr / norms
    return arr


def train_projection(embeddings: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    mean = embeddings.mean(axis=0)
    centered = embeddings - mean

    print("[D] Running SVD...")
    _, S, Vt = np.linalg.svd(centered, full_matrices=False)
    W = Vt[:OUTPUT_DIM].T  # [384, 128]

    var_total = float((S ** 2).sum())
    var_kept = float((S[:OUTPUT_DIM] ** 2).sum())
    print(f"[D] Variance retained in {OUTPUT_DIM} comps: {var_kept / var_total:.4f}")

    matrix = W.T.astype(np.float32)              # [128, 384]
    bias = (-W.T @ mean).astype(np.float32)      # [128]
    return matrix, bias


def save_binary(matrix: np.ndarray, bias: np.ndarray, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "wb") as f:
        f.write(np.uint32(INPUT_DIM).tobytes())
        f.write(matrix.tobytes(order="C"))
        f.write(bias.tobytes(order="C"))
    print(f"[D] Wrote {path} ({path.stat().st_size} bytes)")


def main() -> None:
    corpus = build_corpus()
    print(f"[D] Corpus size: {len(corpus)} lines")
    if len(corpus) < 500:
        print("[D] WARNING: corpus too small, PCA may be unstable", file=sys.stderr)

    embeddings = embed_corpus(corpus)
    matrix, bias = train_projection(embeddings)
    save_binary(matrix, bias, OUT_PATH)

    # Verify read-back
    print("[D] Verifying binary layout...")
    raw = OUT_PATH.read_bytes()
    input_dim = int.from_bytes(raw[:4], "little")
    assert input_dim == INPUT_DIM, f"bad input_dim {input_dim}"
    mat = np.frombuffer(raw[4 : 4 + 128 * INPUT_DIM * 4], dtype=np.float32).reshape(128, INPUT_DIM)
    b = np.frombuffer(raw[4 + 128 * INPUT_DIM * 4 :], dtype=np.float32)
    assert b.shape == (128,), f"bad bias shape {b.shape}"

    # Sanity check
    test_text = "a database connection failed during peak traffic"
    test_emb = next(iter(TextEmbedding(model_name=MODEL_NAME).embed([test_text])))
    y = mat @ np.asarray(test_emb, dtype=np.float32) + b
    print(f"[D] Test projection output norm (pre-Rust-normalize): {np.linalg.norm(y):.4f}")
    print("[D] DONE")


if __name__ == "__main__":
    main()
