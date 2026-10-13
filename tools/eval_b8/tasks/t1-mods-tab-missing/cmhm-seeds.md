# T1 CMHM seed entries (cmhm arm only — learned into an isolated `b8-t1` context before the run)

1. state: "CONTROL Resonant: MODS tab not showing in the options menu"
   action: "removed stray winmm.dll copies inside crmods\\"
   outcome: "MODS tab appeared; crloader.log shows 2 loaded, 0 failed"
   prediction_error: 0.9
2. state: "crloader mod layout"
   action: "recorded loader at root (winmm.dll + crmods\\), logs at crloader.log and crmods\\CRModMenu\\modsui.log"
   outcome: "layout known for future diagnosis"
   prediction_error: 0.2
