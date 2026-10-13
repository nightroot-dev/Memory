# T7 CMHM seed entries (cmhm arm only — isolated `b8-t7` context)

1. state: "gate score looks wrong on unclamped inputs"
   action: "traced verifiability term to raw caller-supplied authority"
   outcome: "clamp gate inputs before scoring"
   prediction_error: 0.7
2. state: "stale context served results"
   action: "checked sleeping-context handling in gate inspection"
   outcome: "unloaded contexts must never serve as live; ensure load first"
   prediction_error: 0.7
