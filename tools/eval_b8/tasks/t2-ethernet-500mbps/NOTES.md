# T2 NOTES.md (notes-file arm only)

# Direct-cable throughput debugging (learned 2026-10-10)
- 500 Mbps on a gigabit Realtek link is NOT a standard step → suspect
  Gigabit Lite / Green Ethernet power-saving downgrade, not the cable.
- Check adapter Advanced properties: Speed & Duplex forced vs Auto;
  Energy Efficient Ethernet on/off; update the inbox driver from Realtek.
- Verify with iperf3 both directions; healthy gigabit reads ~940+ Mbps.
- iperf3 pitfall: `iperf3.exe` must run with `.\` prefix or full path from
  its own folder, else PowerShell reports "not recognized".
