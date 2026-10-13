# T7 NOTES.md (notes-file arm only)

# API-surface pitfalls (learned from audit)
- Gate inputs must be clamped before scoring: unclamped authority feeds the
  verifiability term directly (check the gate math first).
- Sleeping/idle contexts must never serve as live: gate inspection on an
  unloaded context reads zeros and misfires.
- Manifest parsing must be section-aware (`[package]` vs `[dependencies]`).
- Novelty is clamped to [0,1]; anything wider breaks the gate product.
