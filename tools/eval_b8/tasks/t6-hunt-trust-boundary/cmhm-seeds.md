# T6 CMHM seed entries (cmhm arm only — isolated `b8-t6` context)

1. state: "handler trusts caller-supplied role for privilege"
   action: "traced authority decision to the unauthenticated caller_role input"
   outcome: "authority bypass confirmed; roles must be authenticated server-side"
   prediction_error: 0.9
2. state: "vectors compared without checking model identity"
   action: "checked model_id plumbing for silent fallbacks"
   outcome: "cross-geometry comparison gives silently wrong scores; match identity first"
   prediction_error: 0.8
