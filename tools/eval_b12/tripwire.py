#!/usr/bin/env python3
"""B12 registry integrity tripwire.

  python tools/eval_b12/tripwire.py mark <label>     # append display_name set
  python tools/eval_b12/tripwire.py check <before-label> <after-label>  # diff

Records the display_name list of data/registry.toml around every cmhm arm.
Any unexplained disappearance = clobber = arm VOID (re-run once; second
occurrence aborts B12) - b12-design §6. With the c48ee4a fix the list may
only GROW or stay; Deleted tombstones count as intact entries.
"""
import json
import pathlib
import re
import sys

REG = pathlib.Path(r"C:\Games\Projects\Memory\data\registry.toml")
LOG = pathlib.Path(r"C:\Games\Projects\Memory\tools\eval_b12\runs\integrity.log")


def entries() -> dict:
    text = REG.read_text(encoding="utf-8")
    out = {}
    for m in re.finditer(r"display_name\s*=\s*\"([^\"]*)\"", text):
        out[m.group(1)] = True
    return out


def cmd_set(ar, labels):
    stamp = {ar: entries()}
    LOG.parent.mkdir(parents=True, exist_ok=True)
    with LOG.open("a", encoding="utf-8") as f:
        f.write(json.dumps(stamp, sort_keys=True) + "\n")
    print(ar, len(stamp[ar]), "display_names:", sorted(stamp[ar]))


def main():
    args = sys.argv[1:]
    if args[0] == "mark":
        cmd_set(*args)
    elif args[0] == "check":
        stamps = {}
        for line in LOG.read_text(encoding="utf-8").splitlines():
            stamps.update(json.loads(line))
        before = set(stamps[args[1]]) if args[1] in stamps else None
        after = set(stamps[args[2]]) if args[2] in stamps else None
        if before is None or after is None:
            print("MISSING MARKS"); sys.exit(2)
        gone = before - after
        if gone:
            print("CLOBBER DETECTED: entries disappeared:", sorted(gone)); sys.exit(1)
        print(f"intact: {len(before)} -> {len(after)} (grew/kept, nothing vanished)")
    else:
        print("usage: mark <label> | check <before> <after>"); sys.exit(2)


if __name__ == "__main__":
    main()
