# T3 — OpenBot repo exploration + desktop shortcut

## Prompt (all arms)

> Look at the repository in `<fixture>/OpenBot` (GitHub: CopilotKit/OpenBot).
> Report what it is and name its main components. Then create a desktop
> shortcut for launching it — put the shortcut in `<work>/Desktop/` (do NOT
> touch the real desktop) pointing at the repo's entry point or README.
> Report: project summary, components found, shortcut path and target.

## Starting state

Cloned repo as shipped (runner fetches on demand: `git clone
https://github.com/CopilotKit/OpenBot.git`, pinned hash
`6d86ba47369c46e1216cbf9f4ee4ce4840d4db90`, 2026-10-13; the clone itself is
git-ignored, not committed). No other info.

## Verifier (deterministic)

1. Report names at least 3 top-level directories that actually exist under
   `fixtures/OpenBot/` (checked mechanically against the fixture).
2. A shortcut file (`.lnk` or `.url`) exists under `<work>/Desktop/`.
3. The shortcut target resolves to a path inside `fixtures/OpenBot/`.

All three must hold. Tool calls + wall time recorded per arm.
