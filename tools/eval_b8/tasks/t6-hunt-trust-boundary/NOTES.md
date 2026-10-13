# T6 NOTES.md (notes-file arm only)

# Trust-boundary pitfalls (learned from audit, ranked)
1. Authority bypass via attacker-controlled `caller_role`: any handler that
   trusts a caller-supplied role for privilege decisions lets the caller
   grant itself authority. Roles must be authenticated server-side.
2. Silent identity fallback with the wrong `model_id`: vectors compared
   across geometries produce silently wrong scores. Model identity must be
   explicit and matched before any cosine comparison.
