# T5 CMHM seed entries (cmhm arm only — isolated `b8-t5` context)

1. state: "BM25 scores look wrong after document updates"
   action: "checked insert/remove length accounting for double-subtraction"
   outcome: "avgdl corrupts on every re-index; check total_length first"
   prediction_error: 0.8
2. state: "trusted-but-irrelevant node wins retrieval"
   action: "compared score-term magnitudes across the fusion"
   outcome: "authority term was overriding relevance via scale mismatch"
   prediction_error: 0.7
