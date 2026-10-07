#!/usr/bin/env python3
"""Checker for the Session L corpus (docs/CORPUS_PROTOCOL.md).

Verifies, without any embedder:
  1. Every paraphrase query shares ZERO content tokens with its situation.
  2. Every lexical query shares ONLY its rare token (+stopwords) with its
     situation, and no other pair's rare token.
  3. Rare tokens are unique across the whole corpus.
  4. Difficulty distribution of paraphrases.

Usage: python tools/check_corpus_l1.py
Exit 0 = clean. Prints violations otherwise.
"""
import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CORPUS = REPO / "tools" / "corpus_l1.json"

STOP = {
    'the', 'and', 'for', 'was', 'but', 'are', 'has', 'had', 'not', 'with',
    'this', 'that', 'have', 'from', 'they', 'been', 'its', 'into', 'than',
    'when', 'who', 'will', 'after', 'under', 'during', 'before', 'past',
    'some', 'is', 'to', 'on', 'in', 'of', 'at', 'by', 'a', 'an', 'as',
    'or', 'it', 'be', 'why', 'how', 'what', 'did', 'does', 'without',
    'over', 'out', 'are', 'were', 'our', 'your', 'its',
}


def toks(s):
    return {
        t for t in re.split(r'[^a-z0-9]+', s.lower())
        if t and len(t) >= 3 and t not in STOP
    }


def main():
    pairs = json.loads(CORPUS.read_text())
    errors = []
    lex = [p for p in pairs if p['kind'] == 'lexical']
    par = [p for p in pairs if p['kind'] == 'paraphrase']
    print(f"[L1] total={len(pairs)} lexical={len(lex)} paraphrase={len(par)}")

    if len(lex) != 100:
        errors.append(f"lexical count is {len(lex)}, want 100")
    if len(par) != 100:
        errors.append(f"paraphrase count is {len(par)}, want 100")

    # Rare-token uniqueness across the whole corpus.
    seen_rare = {}
    for p in pairs:
        for tok in toks(p['situation']) | toks(p['query']):
            if tok.startswith('l1rare'):
                if tok in seen_rare:
                    errors.append(f"rare token {tok} reused (pairs {seen_rare[tok]} and {p['id']})")
                else:
                    seen_rare[tok] = p['id']

    # Lexical: query shares ONLY its rare token with its situation.
    for p in lex:
        q, s = toks(p['query']), toks(p['situation'])
        shared = (q & s) - {t for t in (q & s) if t.startswith('l1rare')}
        if shared:
            errors.append(f"lex {p['id']}: non-rare shared tokens {sorted(shared)}")
        rare_in_q = [t for t in q if t.startswith('l1rare')]
        if len(rare_in_q) != 1 or rare_in_q[0] not in s:
            errors.append(f"lex {p['id']}: query must carry exactly its own rare token")

    # Paraphrase: ZERO shared content tokens.
    for p in par:
        shared = toks(p['query']) & toks(p['situation'])
        if shared:
            errors.append(f"para {p['id']}: shared tokens {sorted(shared)}")

    # Difficulty distribution.
    from collections import Counter
    dist = Counter(p.get('difficulty', '?') for p in par)
    print(f"[L1] difficulty: {dict(dist)}")

    # Actions/outcomes present and non-empty.
    for p in pairs:
        if not p.get('action') or not p.get('outcome'):
            errors.append(f"{p['id']}: missing action/outcome")

    if errors:
        print(f"[L1] FAIL: {len(errors)} violations")
        for e in errors[:30]:
            print(f"  - {e}")
        sys.exit(1)
    print("[L1] PASS: corpus clean")


if __name__ == '__main__':
    main()
