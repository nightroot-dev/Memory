# T2 CMHM seed entries (cmhm arm only — isolated `b8-t2` context)

1. state: "new Cat6 cable tops out at 500 Mbps between two laptops"
   action: "checked adapter report: Realtek link at non-standard 500 Mbps, power saving on"
   outcome: "diagnosed Gigabit Lite power-saving downgrade, not the cable"
   prediction_error: 0.9
2. state: "iperf3.exe not recognized in PowerShell"
   action: "ran with .\\ prefix from its own folder"
   outcome: "iperf ran; both directions ~500 Mbps pre-fix, ~940+ expected post-fix"
   prediction_error: 0.7
