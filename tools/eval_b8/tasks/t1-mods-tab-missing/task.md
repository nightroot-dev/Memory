# T1 — MODS tab missing (reference task)

## Prompt (all arms)

> The game shows no MODS tab in the options menu. The mod setup lives in
> `<fixture>/`. Diagnose why no mods load and fix it so the MODS tab would
> appear. Report: root cause, files changed, and how you would verify.

## Starting state

Fixture dir as shipped: `crloader.log` (0 loaded, 2 failed), `modsui.log`
(tab not injected), two mod folders each containing a stray `winmm.dll`
alongside `mod.json`. No other info.

## Verifier (deterministic, file-state)

1. No `winmm.dll` remains anywhere under `crmods/` (work on a COPY — the
   runner copies the fixture per arm; the pristine fixture must stay intact).
2. Agent's final report names the cause class: stray/duplicate loader DLL
   inside the mod folders (keyword rubric: `winmm` AND (`stray` OR
   `duplicate` OR `copies` OR `inside crmods`)).
3. Agent states the verification: loader log would show `2 loaded, 0 failed`
   and the MODS tab injected.

All three must hold. Tool calls + wall time recorded per arm.
