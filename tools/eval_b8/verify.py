"""B8 deterministic verifier: `verify.py <task-id> <report.md>`.
Exit 0 = pass, 1 = fail (message explains). Keyword rubrics only — the same
bar judges all three arms. File-state checks live in each task.md; this
script covers the report-text rubrics.
"""
import re
import sys

def has(t, *pats):
    return any(re.search(p, t, re.IGNORECASE) for p in pats)

def need(t, groups):
    """Every group (list of alternative patterns) must have >= 1 hit."""
    missing = [g for g in groups if not has(t, *g)]
    return missing

def t1(t):
    return need(t, [
        [r'winmm'],
        [r'stray', r'duplicate', r'copies', r'inside crmods'],
    ])

def t2(t):
    return need(t, [
        [r'500', r'gigabit lite', r'green ethernet', r'energy efficient'],
        [r'power saving', r'energy efficient', r'duplex', r'auto-negotiat'],
        [r'power saving', r'1\.0 gbps full duplex', r'driver', r'940', r'gigabit'],
        [r'not recognized', r'\.\\\\', r'full path'],
    ])

def t4(t):
    return need(t, [
        [r'replay', r're-appl', r'double-appl'],
        [r'idempoten', r'duplicat', r'double-appl', r're-appl'],
        [r'\bwal\b', r'snapshot', r'truncat', r'watermark', r'compaction'],
    ])

def t5(t):
    return need(t, [
        [r'\brrf\b', r'fusion', r'reciprocal'],
        [r'authority', r'total_length', r'avgdl', r'average document length', r'scale', r'mismatch', r'suppress'],
    ])

def t6(t):
    return need(t, [
        [r'caller_role', r'caller role', r'bypass', r'privilege'],
        [r'model_id'],
    ])

def _t7_area(t, pats, path_hint):
    if not has(t, *pats):
        return False
    return bool(re.search(r'cmhm_mcp/src|context|runtime|tools\.rs|engine\.rs|config\.rs', t, re.IGNORECASE))

def t7(t):
    areas = [
        _t7_area(t, [r'clamp'], True),
        _t7_area(t, [r'sleep', r'wake', r'evict', r'loaded'], True),
        _t7_area(t, [r'mutex', r'poison', r'lock'], True),
        _t7_area(t, [r'manifest', r'cargo\.toml'], True),
        _t7_area(t, [r'novelty'], True),
    ]
    return [] if sum(areas) >= 2 else [['<2 of 5 api-surface areas with file paths']]

def t3_file_state(workdir):
    """Shortcut checks need the work dir; handled by the runner doc, not here."""
    return []

CHECKS = {
    't1': t1, 't1-mods-tab-missing': t1,
    't2': t2, 't2-ethernet-500mbps': t2,
    't4': t4, 't4-hunt-wal-replay': t4,
    't5': t5, 't5-hunt-bm25-scoring': t5,
    't6': t6, 't6-hunt-trust-boundary': t6,
    't7': t7, 't7-hunt-api-surface': t7,
}

def main():
    if len(sys.argv) != 3 or sys.argv[1] not in CHECKS:
        print('usage: verify.py <t1|t2|t4|t5|t6|t7> <report.md>  (t3 is file-state, see task.md)')
        return 2
    text = open(sys.argv[2], encoding='utf-8', errors='replace').read()
    missing = CHECKS[sys.argv[1]](text)
    if missing:
        print('FAIL groups:', missing)
        return 1
    print('PASS')
    return 0

if __name__ == '__main__':
    sys.exit(main())
