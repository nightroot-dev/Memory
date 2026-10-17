param([string]$EnvName)
if ($EnvName -eq 'dev') { throw 'dev requires the local DB override first' }
Write-Host "using $env:DB_URL"
