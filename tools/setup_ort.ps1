# tools/setup_ort.ps1
#
# Ensure ORT_DYLIB_PATH points at a valid onnxruntime.dll (>= 1.24).
# Idempotent: no-op if already valid. Persists to User env so future
# shells inherit it.
#
# Usage:
#   pwsh -File tools/setup_ort.ps1

$ErrorActionPreference = "Stop"

function Test-OrVersion($path) {
    if (-not (Test-Path $path)) { return $false }
    # Primary: PE version resource (reliable, encoding-independent).
    try {
        $prod = (Get-Item $path).VersionInfo.ProductVersion
        $m = [regex]::Match("$prod", '(\d+)\.(\d+)\.(\d+)')
        if ($m.Success) {
            $ver = [version]"$($m.Groups[1].Value).$($m.Groups[2].Value).$($m.Groups[3].Value)"
            Write-Host "  candidate $path has version $ver"
            return ($ver -ge [version]"1.24.0")
        }
    } catch { }
    # Fallback: raw byte scan for an ASCII version string.
    try {
        $bytes = [System.IO.File]::ReadAllBytes($path)
        if ($bytes.Length -lt 1024) { return $false }
        $limit = [Math]::Min($bytes.Length, 2 * 1024 * 1024)
        $sb = New-Object System.Text.StringBuilder
        for ($i = 0; $i -lt $limit; $i++) {
            $b = $bytes[$i]
            if ($b -ge 32 -and $b -le 126) { [void]$sb.Append([char]$b) }
            else { [void]$sb.Append(' ') }
        }
        $txt = $sb.ToString()
        $m = [regex]::Match($txt, '\b(1\.\d{2}\.\d+)\b')
        if (-not $m.Success) { return $false }
        $ver = [version]$m.Groups[1].Value
        Write-Host "  candidate $path has version $ver"
        return ($ver -ge [version]"1.24.0")
    } catch { return $false }
}

if ($env:ORT_DYLIB_PATH -and (Test-OrVersion $env:ORT_DYLIB_PATH)) {
    Write-Host "[setup_ort] ORT_DYLIB_PATH already valid: $env:ORT_DYLIB_PATH"
    exit 0
}

$candidates = @()
$candidates += Get-ChildItem -Recurse "C:\onnxruntime" -Filter "onnxruntime.dll" -ErrorAction SilentlyContinue | Select-Object -ExpandProperty FullName
if ($env:USERPROFILE) {
    $candidates += Get-ChildItem -Recurse "$env:USERPROFILE\scoop\apps\onnxruntime" -Filter "onnxruntime.dll" -ErrorAction SilentlyContinue | Select-Object -ExpandProperty FullName
}
$candidates += "C:\Windows\System32\onnxruntime.dll"

foreach ($c in $candidates) {
    if ($c -and (Test-OrVersion $c)) {
        [Environment]::SetEnvironmentVariable("ORT_DYLIB_PATH", $c, "User")
        $env:ORT_DYLIB_PATH = $c
        Write-Host "[setup_ort] set User ORT_DYLIB_PATH = $c"
        exit 0
    }
}

Write-Host "[setup_ort] no valid ORT found; downloading latest release..."
New-Item -ItemType Directory -Force -Path "C:\onnxruntime" | Out-Null
Push-Location "C:\onnxruntime"
try {
    $rel = Invoke-RestMethod "https://api.github.com/repos/microsoft/onnxruntime/releases/latest"
    $asset = ($rel.assets | Where-Object { $_.name -like "onnxruntime-win-x64-*.zip" })[0]
    if (-not $asset) { throw "no win-x64 zip in latest release" }
    Write-Host "  downloading $($asset.name)"
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $asset.name
    Expand-Archive -Path $asset.name -DestinationPath . -Force
    $dll = Get-ChildItem -Recurse -Filter "onnxruntime.dll" | Select-Object -First 1 -ExpandProperty FullName
    if (-not $dll) { throw "extracted archive has no onnxruntime.dll" }
    [Environment]::SetEnvironmentVariable("ORT_DYLIB_PATH", $dll, "User")
    $env:ORT_DYLIB_PATH = $dll
    Write-Host "[setup_ort] set User ORT_DYLIB_PATH = $dll"
} finally { Pop-Location }
