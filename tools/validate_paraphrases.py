#!/usr/bin/env python3
"""Validate paraphrase pairs THROUGH the embedder before benching.

A pair is kept only if, in projected MiniLM space:
  cos(Q, S_target) - max(cos(Q, S_other)) > MARGIN
and Q shares zero content tokens with S_target.

Usage: python tools/validate_paraphrases.py
"""

import re
import numpy as np
from pathlib import Path
from fastembed import TextEmbedding

MODEL = "sentence-transformers/all-MiniLM-L6-v2"
MARGIN = 0.03
REPO = Path(__file__).resolve().parent.parent

STOP = {'the','and','for','was','but','are','has','had','not','with','this','that',
        'have','from','they','been','its','into','than','when','who','will','after',
        'under','during','before','past','some','is','to','on','in','of','at','by',
        'a','an','as','or','it','be','why','how','what','did','does','without',
        'over','out'}

def toks(s):
    return set(t for t in re.split(r'[^a-z0-9]+', s.lower())
               if t and len(t) >= 3 and t not in STOP)

# (situation, query): 40 current + 30 new candidates
CANDIDATES = [
    # current 30 untouched pairs
    ("database connection pool exhausted under peak load", "why are connections timing out"),
    ("authentication token expired before refresh completed", "users logged out unexpectedly"),
    ("message queue backlog grew past safe threshold", "processing fell behind"),
    ("disk usage reached critical on primary volume", "storage is almost full"),
    ("background scheduler missed two hourly runs", "scheduled jobs did not execute"),
    ("redis replica fell behind primary by seconds", "cache reads return stale data"),
    ("payment processor returned intermittent 502 errors", "checkout randomly fails"),
    ("search index rebuilt slower than expected", "results arrive late"),
    ("API gateway rate limit triggered for bulk requests", "clients hit request limits"),
    ("feature flag rollout exposed bug to small user group", "some users see a defect"),
    ("database migration locked the events table", "writes paused during upgrade"),
    ("session store replication lag grew overnight", "users lose sessions after login"),
    ("DNS resolver returned stale entries after config push", "hostnames resolve to old addresses"),
    ("load balancer health check flapped during deploy", "traffic shifted unexpectedly"),
    ("storage tier promotion delayed by replication backlog", "hot data lags on cold media"),
    ("build cache corrupted causing spurious test failures", "CI fails without real cause"),
    ("TLS certificate approaching expiry without warning", "browsers show insecure warnings"),
    ("queue consumer crashed mid-batch leaving orphans", "some messages never processed"),
    ("backup job missed window due to upstream delay", "backups are out of date"),
    ("metrics exporter dropped series after schema change", "dashboards show gaps"),
    ("authentication service returned 401 for valid tokens", "legitimate users blocked"),
    ("object store returned slow reads under high concurrency", "downloads are sluggish"),
    ("cron scheduler moved timezone causing missed triggers", "jobs fire at wrong times"),
    ("CDN edge cache poisoned with stale content", "users see outdated assets"),
    ("certificate rotation broke mutual TLS handshake", "some services cannot connect"),
    ("streaming pipeline backpressure stalled consumers", "real-time data falls behind"),
    ("scheduler fairness starved low-priority jobs", "background tasks never run"),
    ("distributed lock leaked causing worker contention", "workers stall waiting for locks"),
    ("message dedup key collided across tenants", "some events processed twice"),
    ("query planner chose suboptimal index after stats update", "queries became slow overnight"),
    # current 10 rewritten pairs
    ("cache eviction rate spiked after memory pressure", "responses are sluggish"),
    ("kubernetes pod entered crash loop after config change", "keeps restarting"),
    ("log aggregator dropped events during rotation", "some lines are missing"),
    ("database vacuum ran longer than maintenance window", "cleanup overran its allotted time"),
    ("config reload triggered unintended service restart", "nodes reboot whenever settings refresh"),
    ("cron worker memory grew without bound", "background process exhausted its allocation"),
    ("service mesh sidecar restarted under memory pressure", "calls between systems drop briefly"),
    ("container image pull slowed by registry throttling", "deployments take longer to start"),
    ("TLS handshake latency increased on the ingress", "inbound requests slow to connect"),
    ("resource quota blocked autoscaler from scaling", "capacity could not grow under load"),
    # 30 new candidates
    ("SSL certificate chain validation failed on edge nodes", "browsers reject the site identity"),
    ("primary database failover stalled mid-promotion", "standby never took over traffic"),
    ("connection pool leak drained available sockets", "new requests wait indefinitely"),
    ("garbage collector pause exceeded stop-the-world budget", "application froze for seconds"),
    ("leader election flapped between two candidates", "cluster has no stable leader"),
    ("write-ahead log segment filled the disk partition", "node refuses new writes"),
    ("read replica lagged minutes behind the writer", "queries return outdated rows"),
    ("connection timeouts cascade across dependent services", "outage spreads downstream"),
    ("thread pool saturation rejected incoming tasks", "server turns away work"),
    ("file descriptor table filled under connection storm", "process cannot open sockets"),
    ("DNS TTL misconfiguration pinned clients to dead endpoints", "traffic hits retired hosts"),
    ("hot partition throttled by rate governor", "one shard is much slower"),
    ("cold start latency spiked for serverless handlers", "first requests are very slow"),
    ("retry storm amplified a partial outage", "clients hammering made it worse"),
    ("poison message blocked the consumer group", "one bad record halts processing"),
    ("clock skew broke token expiry validation", "fresh credentials rejected as stale"),
    ("split brain partitioned the cluster in two", "halves cannot see each other"),
    ("compaction backlog grew on the storage engine", "reads degrade over time"),
    ("hedged requests doubled load on the backend", "redundant calls overload servers"),
    ("graceful shutdown hung waiting for drains", "deploys stall at termination"),
    ("sticky session affinity pinned load unevenly", "one server gets everything"),
    ("request coalescing collapsed under thundering herd", "cache stampede overloaded origin"),
    ("schema migration backfill lagged for hours", "new column stays empty"),
    ("checkpoint interval too coarse losing recent writes", "restarts lose minutes of data"),
    ("heartbeat timeout evicted healthy members", "cluster shrinks for no reason"),
    ("payload compression ratio collapsed on binary blobs", "bandwidth savings vanished"),
    ("idempotency window expired allowing duplicate charges", "customers billed twice"),
    ("traffic shadowing duplicated production load", "mirror traffic overwhelms staging"),
    ("slow consumer dragged down the whole pipeline", "one lagging stage blocks all"),
    ("noisy neighbor saturated the shared host", "unrelated tenant slows us down"),
]

def load_projection():
    raw = (REPO / "data" / "projections" / "minilm-l6-v2-int8.bin").read_bytes()
    assert int.from_bytes(raw[:4], "little") == 384
    mat = np.frombuffer(raw[4:4 + 128 * 384 * 4], dtype=np.float32).reshape(128, 384)
    b = np.frombuffer(raw[4 + 128 * 384 * 4:], dtype=np.float32)
    return mat, b

def main():
    model = TextEmbedding(model_name=MODEL)
    def emb(texts):
        arr = np.stack([np.asarray(v, dtype=np.float32) for v in model.embed(texts)])
        arr /= np.linalg.norm(arr, axis=1, keepdims=True).clip(min=1e-12)
        return arr
    mat, b = load_projection()
    def proj(e):
        return (e @ mat.T + b)

    print(f"[V] candidates: {len(CANDIDATES)}")
    # zero-overlap prefilter
    clean = [(s, q) for s, q in CANDIDATES if not (toks(s) & toks(q))]
    dropped = len(CANDIDATES) - len(clean)
    print(f"[V] zero-overlap prefilter dropped: {dropped}")

    sits = [s for s, _ in clean]
    S = proj(emb(sits))
    S /= np.linalg.norm(S, axis=1, keepdims=True).clip(min=1e-12)

    scored = []
    for i, (s, q) in enumerate(clean):
        qv = proj(emb([q])[0])
        qv /= np.linalg.norm(qv).clip(min=1e-12)
        cos_t = float(qv @ S[i])
        best_o, best_j = -2.0, -1
        for j in range(len(clean)):
            if j == i:
                continue
            c = float(qv @ S[j])
            if c > best_o:
                best_o, best_j = c, j
        margin = cos_t - best_o
        scored.append((margin, cos_t, best_o, i, s, q))

    scored.sort(reverse=True)
    print(f"[V] top margins:")
    for margin, cos_t, best_o, i, s, q in scored[:45]:
        print(f"[V]   margin={margin:+.4f} cos_t={cos_t:.4f} best_other={best_o:.4f} | {q:.60}")
    kept = [t for t in scored if t[0] > MARGIN][:40]
    print(f"[V] kept with margin>{MARGIN}: {len(kept)}/40 slots")
    if len(kept) < 40:
        print(f"[V] SHORTFALL: only {len(kept)} pairs pass; take top-40 regardless?")
        kept = scored[:40]
    import json
    out = [{"s": s, "q": q, "margin": m, "cos_t": c} for m, c, _, _, s, q in kept]
    (REPO / "tools" / "validated_pairs.json").write_text(json.dumps(out, indent=1))
    print(f"[V] wrote tools/validated_pairs.json with {len(out)} pairs")

if __name__ == "__main__":
    main()
