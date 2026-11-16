#!/usr/bin/env python3
"""Fine-tune the Phase 5 experience reranker and export quantized ONNX.

The base model is a sentence-transformers CrossEncoder:
ModernBERT -> CLS pooling -> Dense(512) -> LayerNorm -> Dense(1).

Training data is the JSONL emitted by tools/mine_git_triplets.py. Each row
contributes one positive pair (label 1) and one hard-negative pair (label 0).
The script trains with an explicit PyTorch loop (AdamW + BCEWithLogits),
measures Recall@k on the held-out split, then exports
``model_quantized.onnx`` + ``tokenizer.json`` for the Rust ``ort`` runtime.

Usage:
    python tools/train_experience_reranker.py --data tools/phase5_data
    python tools/train_experience_reranker.py --data tools/phase5_data --smoke-test
"""

from __future__ import annotations

import argparse
import json
import random
import time
from pathlib import Path

import torch

from sentence_transformers import CrossEncoder


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_DATA = REPO_ROOT / "tools" / "phase5_data"
DEFAULT_OUT = DEFAULT_DATA / "reranker"


def load_jsonl(path: Path) -> list[dict]:
    rows: list[dict] = []
    with path.open("r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            if all(k in row and row[k] for k in ("query", "positive", "negative")):
                rows.append(row)
    return rows


def trigger_only(payload: str) -> str:
    """The runtime scores only the trigger signature, never the decision payload.

    The miner stores ``"trigger :: decision"`` so the corpus keeps both halves;
    the cross-encoder must learn the same ``(query, trigger)`` pair the Rust
    stage-2 path will feed it at inference time.
    """
    trigger = payload.split(" :: ", 1)[0].strip()
    return trigger or payload.strip()


def make_pairs(rows: list[dict]) -> list[tuple[str, str, float]]:
    pairs: list[tuple[str, str, float]] = []
    for row in rows:
        pairs.append((row["query"], trigger_only(row["positive"]), 1.0))
        pairs.append((row["query"], trigger_only(row["negative"]), 0.0))
    return pairs


def evaluate_recall(
    model: CrossEncoder,
    rows: list[dict],
    extra_negatives: int = 24,
    seed: int = 13,
    batch_size: int = 64,
) -> dict:
    """Rank each query's positive against its hard negative + sampled negatives."""
    rng = random.Random(seed)
    pool = [trigger_only(r["positive"]) for r in rows] + [
        trigger_only(r["negative"]) for r in rows
    ]
    hits = {1: 0, 3: 0, 5: 0}
    reciprocal = 0.0
    usable = 0

    for row in rows:
        candidates = [trigger_only(row["positive"]), trigger_only(row["negative"])]
        while len(candidates) < extra_negatives + 2:
            candidates.append(rng.choice(pool))
        # Shuffle so the positive is not always first.
        indexed = list(enumerate(candidates))
        rng.shuffle(indexed)
        pairs = [(row["query"], text) for _, text in indexed]
        scores = model.predict(pairs, batch_size=batch_size, show_progress_bar=False)
        order = sorted(range(len(scores)), key=lambda i: float(scores[i]), reverse=True)
        positive_pos = next(
            (rank for rank, idx in enumerate(order) if indexed[idx][0] == 0), None
        )
        if positive_pos is None:
            continue
        usable += 1
        for k in hits:
            if positive_pos < k:
                hits[k] += 1
        reciprocal += 1.0 / (positive_pos + 1)

    if usable == 0:
        return {"queries": 0, "recall@1": 0.0, "recall@3": 0.0, "recall@5": 0.0, "mrr": 0.0}
    return {
        "queries": usable,
        "recall@1": round(hits[1] / usable, 4),
        "recall@3": round(hits[3] / usable, 4),
        "recall@5": round(hits[5] / usable, 4),
        "mrr": round(reciprocal / usable, 4),
    }


def train_model(model: CrossEncoder, rows: list[dict], args: argparse.Namespace) -> dict:
    """Train with an explicit PyTorch loop.

    sentence-transformers' deprecated ``fit`` routes through ``datasets``,
    whose dill pickling is broken on Python 3.14 (pickle's ``_batch_setitems``
    gained an ``obj`` argument). The loop also makes the loss and schedule
    explicit in the manifest.
    """
    pairs = make_pairs(rows)
    device = next(model.parameters()).device
    tokenizer = model.tokenizer
    optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=0.01)
    steps_per_epoch = max(1, (len(pairs) + args.batch_size - 1) // args.batch_size)
    total_steps = steps_per_epoch * args.epochs
    warmup = min(args.warmup_steps, max(1, total_steps // 10))

    def lr_lambda(step: int) -> float:
        if step < warmup:
            return (step + 1) / warmup
        return max(0.0, (total_steps - step) / max(1, total_steps - warmup))

    scheduler = torch.optim.lr_scheduler.LambdaLR(optimizer, lr_lambda)
    loss_fct = torch.nn.BCEWithLogitsLoss()
    use_amp = device.type == "cuda"
    rng = random.Random(17)
    step = 0
    losses: list[float] = []

    model.train()
    for epoch in range(args.epochs):
        rng.shuffle(pairs)
        running = 0.0
        for start in range(0, len(pairs), args.batch_size):
            batch = pairs[start : start + args.batch_size]
            encoded = tokenizer(
                [query for query, _, _ in batch],
                [doc for _, doc, _ in batch],
                padding=True,
                truncation=True,
                max_length=args.max_length,
                return_tensors="pt",
            )
            features = {key: value.to(device) for key, value in encoded.items()}
            labels = torch.tensor(
                [label for _, _, label in batch], dtype=torch.float32, device=device
            )
            optimizer.zero_grad(set_to_none=True)
            with torch.autocast(device_type="cuda", dtype=torch.bfloat16, enabled=use_amp):
                scores = model(features)["scores"].view(-1)
                loss = loss_fct(scores.float(), labels)
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            optimizer.step()
            scheduler.step()
            losses.append(loss.item())
            running += loss.item()
            step += 1
            if step % 50 == 0:
                print(
                    f"epoch {epoch + 1}/{args.epochs} step {step}/{total_steps} "
                    f"loss {running / 50:.4f}",
                    flush=True,
                )
                running = 0.0
    model.eval()
    return {
        "steps": step,
        "steps_per_epoch": steps_per_epoch,
        "warmup_steps": warmup,
        "final_loss": round(losses[-1], 6) if losses else None,
        "mean_loss_last_50": round(sum(losses[-50:]) / max(1, len(losses[-50:])), 6),
    }


def export_onnx(model: CrossEncoder, out_dir: Path, max_length: int, base_model: str) -> dict:
    import onnx
    import onnxruntime as ort
    from onnxruntime.quantization import QuantType, quantize_dynamic
    from transformers import AutoConfig

    out_dir.mkdir(parents=True, exist_ok=True)
    fp32_path = out_dir / "model.onnx"
    quant_path = out_dir / "model_quantized.onnx"

    class Wrapper(torch.nn.Module):
        def __init__(self, cross_encoder: CrossEncoder) -> None:
            super().__init__()
            self.cross_encoder = cross_encoder

        def forward(self, input_ids: torch.Tensor, attention_mask: torch.Tensor) -> torch.Tensor:
            features = {"input_ids": input_ids, "attention_mask": attention_mask}
            # ``CrossEncoder.model`` is the ModernBERT backbone only; the full
            # container runs the CLS pooling + Dense/LayerNorm/Dense head.
            return self.cross_encoder(features)["scores"]

    model.eval()
    device = next(model.parameters()).device
    wrapper = Wrapper(model).to(device).eval()
    dummy = model.tokenizer(
        "query: build fails with E0382",
        "borrow of moved value in spawn_blocking",
        return_tensors="pt",
        truncation=True,
        max_length=max_length,
        padding=True,
    )
    input_ids = dummy["input_ids"].to(device)
    attention_mask = dummy["attention_mask"].to(device)

    with torch.no_grad():
        torch_scores = wrapper(input_ids, attention_mask).detach().cpu()

    torch.onnx.export(
        wrapper,
        (input_ids, attention_mask),
        str(fp32_path),
        input_names=["input_ids", "attention_mask"],
        output_names=["logits"],
        dynamic_axes={
            "input_ids": {0: "batch", 1: "sequence"},
            "attention_mask": {0: "batch", 1: "sequence"},
            "logits": {0: "batch"},
        },
        opset_version=18,
        do_constant_folding=True,
    )
    # Torch writes large weights to an external .data file; inline them so the
    # fp32 artifact is one self-contained .onnx. ORT's dynamic quantizer also
    # calls replace_gemm_with_matmul() and then re-runs shape inference against
    # the exported value_info, which leaves a stale (512) vs (1) conflict in
    # the Dense(1) head. Pre-apply the rewrite and refresh value_info so the
    # quantizer's own pass is a no-op.
    from onnxruntime.quantization.onnx_model import ONNXModel

    fp32_model = onnx.load(str(fp32_path))
    ort_model = ONNXModel(fp32_model)
    ort_model.replace_gemm_with_matmul()
    del ort_model.model.graph.value_info[:]
    refreshed = onnx.shape_inference.infer_shapes(ort_model.model, strict_mode=False)
    onnx.save_model(refreshed, str(fp32_path))
    for stale in out_dir.glob("*.data"):
        stale.unlink()
    quantize_dynamic(
        str(fp32_path),
        str(quant_path),
        weight_type=QuantType.QInt8,
        per_channel=True,
        reduce_range=True,
    )

    session = ort.InferenceSession(str(quant_path), providers=["CPUExecutionProvider"])
    onnx_scores = session.run(
        None,
        {
            "input_ids": input_ids.detach().cpu().numpy(),
            "attention_mask": attention_mask.detach().cpu().numpy(),
        },
    )[0]
    max_abs_diff = float(
        torch.max(torch.abs(torch_scores.reshape(-1) - torch.tensor(onnx_scores).reshape(-1))).item()
    )
    model.tokenizer.save_pretrained(str(out_dir))
    # CrossEncoder's fast tokenizer does not always emit special_tokens_map.json,
    # but fastembed's TokenizerFiles requires it. Write the map explicitly.
    special_map_path = out_dir / "special_tokens_map.json"
    if not special_map_path.is_file():
        special_map_path.write_text(
            json.dumps(model.tokenizer.special_tokens_map, indent=2) + "\n",
            encoding="utf-8",
            newline="\n",
        )
    # fastembed's user-defined reranker reads config.json for pad_token_id and
    # tokenizer_config.json for model_max_length/pad_token.
    AutoConfig.from_pretrained(base_model).save_pretrained(str(out_dir))
    return {
        "fp32_onnx": fp32_path.name,
        "quantized_onnx": quant_path.name,
        "fp32_bytes": fp32_path.stat().st_size,
        "quantized_bytes": quant_path.stat().st_size,
        "input_names": ["input_ids", "attention_mask"],
        "output_name": "logits",
        "max_abs_diff_vs_torch": round(max_abs_diff, 6),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", default=str(DEFAULT_DATA))
    parser.add_argument("--out", default=str(DEFAULT_OUT))
    parser.add_argument("--model", default="cross-encoder/ettin-reranker-68m-v1")
    parser.add_argument("--epochs", type=int, default=3)
    parser.add_argument("--batch-size", type=int, default=16)
    parser.add_argument("--lr", type=float, default=2e-5)
    parser.add_argument("--warmup-steps", type=int, default=200)
    parser.add_argument("--max-length", type=int, default=256)
    parser.add_argument("--extra-negatives", type=int, default=24)
    parser.add_argument("--limit", type=int, default=0, help="cap train rows (smoke tests)")
    parser.add_argument("--smoke-test", action="store_true", help="tiny run, no export")
    parser.add_argument(
        "--export-only",
        action="store_true",
        help="load checkpoint.pt from --out and export without training",
    )
    parser.add_argument("--device", default="cuda" if torch.cuda.is_available() else "cpu")
    args = parser.parse_args()

    data_dir = Path(args.data)
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    train = load_jsonl(data_dir / "train.jsonl")
    val = load_jsonl(data_dir / "val.jsonl")
    test = load_jsonl(data_dir / "test.jsonl")
    if not train:
        print(f"no training rows in {data_dir / 'train.jsonl'}")
        return 1
    if args.limit:
        train = train[: args.limit]
    if args.smoke_test:
        train = train[: max(8, min(32, len(train)))]
        val = val[: max(4, min(8, len(val)))]
        args.epochs = 1

    print(
        f"rows: train={len(train)} val={len(val)} test={len(test)}; "
        f"device={args.device}; cuda={torch.cuda.is_available()}",
        flush=True,
    )
    started = time.time()
    model = CrossEncoder(
        args.model,
        num_labels=1,
        max_length=args.max_length,
        device=args.device,
    )

    if args.export_only:
        checkpoint = out_dir / "checkpoint.pt"
        if not checkpoint.is_file():
            print(f"--export-only needs {checkpoint}; run a full training pass first")
            return 1
        model.load_state_dict(torch.load(checkpoint, map_location=args.device))
        model.eval()
        export = export_onnx(model, out_dir, args.max_length, args.model)
        manifest_path = out_dir / "training_manifest.json"
        manifest = (
            json.loads(manifest_path.read_text(encoding="utf-8"))
            if manifest_path.is_file()
            else {}
        )
        manifest["export"] = export
        manifest_path.write_text(
            json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n"
        )
        print(f"export: {export}", flush=True)
        return 0

    base_metrics = evaluate_recall(model, val, extra_negatives=args.extra_negatives) if val else {}
    print(f"base val: {base_metrics}", flush=True)

    train_stats = train_model(model, train, args)
    print(f"train: {train_stats}", flush=True)
    torch.save(model.state_dict(), out_dir / "checkpoint.pt")

    tuned_metrics = evaluate_recall(model, val, extra_negatives=args.extra_negatives) if val else {}
    print(f"tuned val: {tuned_metrics}", flush=True)
    test_metrics = evaluate_recall(model, test, extra_negatives=args.extra_negatives) if test else {}

    manifest = {
        "base_model": args.model,
        "architecture": "ModernBERT -> CLS pooling -> Dense(512) -> LayerNorm -> Dense(1)",
        "trained_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "elapsed_sec": round(time.time() - started, 1),
        "device": args.device,
        "torch": torch.__version__,
        "cuda": torch.cuda.is_available(),
        "hyperparameters": {
            "epochs": args.epochs,
            "batch_size": args.batch_size,
            "lr": args.lr,
            "max_length": args.max_length,
            "warmup_steps": train_stats["warmup_steps"],
            "optimizer": "AdamW(weight_decay=0.01)",
            "loss": "BCEWithLogitsLoss",
            "amp": "bf16 autocast on cuda",
            "seed": 17,
        },
        "train_stats": train_stats,
        "counts": {"train": len(train), "val": len(val), "test": len(test)},
        "metrics": {"base_val": base_metrics, "tuned_val": tuned_metrics, "tuned_test": test_metrics},
    }

    manifest_path = out_dir / "training_manifest.json"
    manifest_path.write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n"
    )

    if not args.smoke_test:
        export = export_onnx(model, out_dir, args.max_length, args.model)
        manifest["export"] = export
        manifest_path.write_text(
            json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n"
        )
        print(f"export: {export}", flush=True)

    print(json.dumps(manifest["metrics"], indent=2), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
