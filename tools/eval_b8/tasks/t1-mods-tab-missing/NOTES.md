# T1 NOTES.md (notes-file arm only — never shown to other arms)

# CONTROL Resonant mod setup (FitGirl repack, RUNE emu)
- Loader: crloader 1.0.0 at game root (`winmm.dll` + `crmods\`).
- Mods live in `crmods\<Name>\` (framework: `crmods\CRModMenu\`).
- Logs: `crloader.log` (root), `crmods\CRModMenu\modsui.log`.
- MODS tab is injected into `OptionsComponent.template.html`; F9 bound when present.

# Known failure (learned 2026-10-11)
- Stray `winmm.dll` copies inside `crmods\` get loaded as mods → remove them.
- Healthy state: `2 loaded, 0 failed` in `crloader.log`.
