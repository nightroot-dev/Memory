# T2 — Ethernet capped at 500 Mbps (diagnosis-only task)

## Prompt (all arms)

> A new Cat 6 cable (box says 10 Gbps) connects two Windows laptops directly,
> but iperf3 tops out around 500 Mbps both ways. The evidence is in
> `<fixture>/` (symptom summary, iperf transcript, adapter report). Diagnose
> the cap and give the remediation steps with how to verify each one. Report:
> root cause, remediation in order, and expected post-fix numbers.

## Starting state

Fixture files as shipped. No other info.

## Verifier (keyword rubric on the final diagnosis — deterministic)

Must name ALL of (case-insensitive):
1. The non-standard link speed: `500` AND (`negotiat` OR `link speed`), OR the
   words `gigabit lite` / `green ethernet` / `energy efficient`.
2. A power/negotiation cause: `power saving` OR `energy efficient` OR
   `duplex` OR `auto-negotiat`.
3. Remediation containing at least two of: disable power saving / force
   `1.0 Gbps full duplex` / update Realtek driver / retest with iperf
   (`940` or `gigabit` as the expected number).
4. The iperf path pitfall: running `iperf3.exe` from the wrong directory
   (`not recognized` / `use .\\` / full path).

All four must hold. Tool calls + wall time recorded per arm.
