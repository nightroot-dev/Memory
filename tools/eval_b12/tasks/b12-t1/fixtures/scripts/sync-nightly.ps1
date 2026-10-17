$conf = Read-Config "$PSScriptRoot\..\conf\sync.toml"
if (-not $conf.sync.enabled) { exit 0 }   # silent no-op when disabled
Start-Sync $conf.targets
Write-AppLog "$PSScriptRoot\..\logs\sync.log" "completed run=$conf.run_id"
