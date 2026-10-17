# B12 task t4 - MakeShadow

Arms share these bodies verbatim; ONLY the arm block differs (README/tools eval_b12).
cmhm arms use context display_name prefix 'b12-t4-cmhm'.

## SESSION 1 PROMPT (body used verbatim for every arm)

# MakeShadow - session 1 (discover)

Work strictly inside this directory: the run work dir.
The build lands in the wrong directory and echoes the wrong value for VAR. Diagnose why the assignment in config/defaults.mk has no effect on the build rule, and report the include-placement fact.
Save your final findings in your reply; the runner saves it as report-s1.md.

## SESSION-1 ARM BLOCKS (append verbatim to the body)

- none:       ARM BLOCK (none): none - just work in this directory.
- self-notes: ARM BLOCK (self-notes): As you work, maintain a file named NOTES.md in your work directory. Write down every fact you learn that would be needed to resume or redo this task later: root causes, file locations, commands, pitfalls, decisions. Keep it concise but complete. Do not write anything outside your work directory.
- cmhm:       ARM BLOCK (cmhm): You have the CMHM memory server. First call context_create with display_name b12-t4-cmhm (context_type 'session') and use ONLY that context for all memory calls. Learn outcome-changing events as you work (per the server instructions; do not spam). Retrievals in this session: at most 2.

## SESSION 2 PROMPT (body used verbatim for every arm)

# MakeShadow - session 2 (use the previous session's findings)

Work strictly inside this directory: the run work dir.
You are a NEW session. A previous session already worked on this task and its findings may exist in this work dir.
Fix Makefile so config/defaults.mk takes effect (edit ONLY Makefile; do not touch defaults.mk or main.c). Then create build/build-note.txt with one line explaining the cause (must mention precedence or placement).
The runner saves your reply as report-s2.md. Do not redo discovery you can trust.

## SESSION-2 ARM BLOCKS (append verbatim to the body)

- none:       (no block - body only)
- self-notes: ARM BLOCK (self-notes): A previous session worked on this same task and may have left NOTES.md in the work directory. Read it first if present.
- cmhm:       ARM BLOCK (cmhm): A previous session on this exact task used the CMHM memory server. Call context_list FIRST, find the context whose display_name starts with b12-t4-cmhm (ignore all others), then memory_retrieve_text (at most 2 calls) before working. Do not touch any other context.

## VERIFIER

Rubric pre-registered in tools/eval_b12/verify.py under this task id.

