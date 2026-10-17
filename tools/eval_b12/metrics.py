#!/usr/bin/env python3
"""B12 metrics + report extraction from opencode.db (read-only).

  python tools/eval_b12/metrics.py <title> [<title> ...]

For each title: locate session_v2 row (prints parity: agent/model), pull
session_message rows, then print:
- tool-call count: rows whose payload has content[] with type=="tool"
  (counts nested execute subcalls the same way B10 did: every tools.<ns>
  invocation inside execute code is counted as a call);
- wall: last - first message timestamp (ms);
- compliance: cmhm tool names seen (context_create/context_list/
  memory_learn/memory_retrieve_text counts);
- final assistant text block (the report body the runner saves).
"""
import json
import sqlite3
import sys

DB = r"file:C:/Users/dbroot/.local/share/opencode/opencode.db?mode=ro"


def rows(cur, sql, args=()):
    cur.execute(sql, args)
    return cur.fetchall()


def main():
    con = sqlite3.connect(DB, uri=True)
    cur = con.cursor()
    for title in sys.argv[1:]:
        rs = rows(cur, "SELECT id, agent, model, time_created, time_updated,"
                       " directory FROM session_v2 WHERE title = ?", (title,))
        if not rs:
            print(f"== {title}: NOT FOUND")
            continue
        for sid, agent, model, t0, t1, dir_ in rs:
            msgs = rows(cur, "SELECT data, time_created FROM session_message"
                             " WHERE session_id = ? ORDER BY time_created", (sid,))
            calls, tool_names, final_text = 0, [], []
            for (data, ts) in msgs:
                try:
                    pl = json.loads(data or "{}")
                except Exception:
                    continue
                for c in pl.get("content", []) or []:
                    if not isinstance(c, dict):
                        continue
                    if c.get("type") == "tool":
                        calls += 1
                        tool_names.append(c.get("name", "?"))
                        if c.get("name") == "execute":
                            code = (c.get("input") or {}).get("code") or ""
                            for m in __import__("re").finditer(r"tools\.[A-Za-z_][\w.]*", code):
                                calls += 1
                                tool_names.append(m.group(0))
                    if c.get("type") == "text" and c.get("role") == "assistant":
                        final_text.append(c.get("text", ""))
            print(f"== {title} [{sid}] agent={agent} model={json.loads(model or '{}').get('id') if model else None} dir={dir_}")
            wall = (t1 or 0) - (t0 or 0)
            print(f"   calls={calls} wall_ms={wall} msgs={len(msgs)}")
            cm = [n for n in tool_names if "cmhm" in n or "memory_" in n or n.startswith("context") or n == "tether"]
            if cm:
                print(f"   cmhm-related: {cm}")
            if final_text:
                print("   -- final assistant text (tail 400 chars) --")
                print("   " + final_text[-1][-400:].replace("\n", "\n   ")[-800:])
    con.close()


if __name__ == "__main__":
    main()
