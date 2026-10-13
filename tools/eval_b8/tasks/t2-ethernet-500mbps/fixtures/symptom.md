# Symptom (observed, no diagnosis included)

- Cable: Dubstep Blaze Cat 6, 1.5 m, box advertises 10 Gbps.
- Setup: two Windows laptops connected directly (no router, no internet).
  Laptop 1: DESKTOP-F08081K. Laptop 2: 169.254.245.70 (link-local).
- Observed: iperf3 both directions tops out around 500 Mbps. Expected ~940+
  Mbps for a gigabit link. Both directions, 15 s runs, 4 parallel streams.
- The cable is new; the user suspects the cable, the adapters, or Windows settings.
