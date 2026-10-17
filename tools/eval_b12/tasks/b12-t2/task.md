# B12 task t2 - IniOverride

Arms share these bodies verbatim; ONLY the arm block differs (README/tools eval_b12).
cmhm arms use context display_name prefix 'b12-t2-cmhm'.

## SESSION 1 PROMPT (body used verbatim for every arm)

# IniOverride - session 1 (discover)

Work strictly inside this directory: the run work dir.
The deployed service reports mode STAGING even though app/app.ini says PROD. Explain the exact mechanism (include handling in app/load.py) and report: the include chain, which file wins, and which keys come from the winning file.
Save your final findings in your reply; the runner saves it as report-s1.md.

## SESSION-1 ARM BLOCKS (append verbatim to the body)

- none:       ARM BLOCK (none): none - just work in this directory.
- self-notes: ARM BLOCK (self-notes): As you work, maintain a file named NOTES.md in your work directory. Write down every fact you learn that would be needed to resume or redo this task later: root causes, file locations, commands, pitfalls, decisions. Keep it concise but complete. Do not write anything outside your work directory.
- cmhm:       ARM BLOCK (cmhm): You have the CMHM memory server. First call context_create with display_name b12-t2-cmhm (context_type 'session') and use ONLY that context for all memory calls. Learn outcome-changing events as you work (per the server instructions; do not spam). Retrievals in this session: at most 2.

## SESSION 2 PROMPT (body used verbatim for every arm)

# IniOverride - session 2 (use the previous session's findings)

Work strictly inside this directory: the run work dir.
You are a NEW session. A previous session already worked on this task and its findings may exist in this work dir.
Make prod runs resolve mode = PROD without deleting or editing app/app.ini or env/overrides.ini. Create env/prod.ini (carrying mode = PROD and maxconn = 250) and change ONLY app/load.py so that when '--env prod' is passed on its command line, prod.ini is applied after any includes. The default behavior (no flag) keeps the same output.
The runner saves your reply as report-s2.md. Do not redo discovery you can trust.

## SESSION-2 ARM BLOCKS (append verbatim to the body)

- none:       (no block - body only)
- self-notes: ARM BLOCK (self-notes): A previous session worked on this same task and may have left NOTES.md in the work directory. Read it first if present.
- cmhm:       ARM BLOCK (cmhm): A previous session on this exact task used the CMHM memory server. Call context_list FIRST, find the context whose display_name starts with b12-t2-cmhm (ignore all others), then memory_retrieve_text (at most 2 calls) before working. Do not touch any other context.

## VERIFIER

Rubric pre-registered in tools/eval_b12/verify.py under this task id.

