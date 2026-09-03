# scripts/verify_phase2.ps1
# Phase 2 Complete Reproducible Verification Battery (Deadlock-Free Native Pipeline)

$logFile = "phase2_verification_evidence.log"
if (Test-Path $logFile) {
    Remove-Item $logFile -Force
}

$global:failedSteps = 0

function Log-Section($title) {
    $line = "=" * 80
    $utcStr = [DateTime]::UtcNow.ToString("yyyy-MM-dd HH:mm:ss UTC")
    $header = "`n$line`n[SECTION] $title`nTimestamp: $utcStr`n$line`n"
    Write-Host $header -ForegroundColor Cyan
    Add-Content -Path $logFile -Value $header
}

function Run-Step($stepName, $cmdLine) {
    $subline = "-" * 80
    $stepHeader = "`n$subline`n[STEP] $stepName`nCommand: $cmdLine`n$subline"
    Write-Host $stepHeader -ForegroundColor Yellow
    Add-Content -Path $logFile -Value $stepHeader

    $tempOut = [System.IO.Path]::GetTempFileName()
    cmd /c "$cmdLine > `"$tempOut`" 2>&1"
    $exitCode = $LASTEXITCODE

    $output = [System.IO.File]::ReadAllText($tempOut)
    Remove-Item $tempOut -Force -ErrorAction SilentlyContinue

    Add-Content -Path $logFile -Value $output
    $exitMsg = "`n[RESULT] Exit Code: $exitCode`n"
    Write-Host $exitMsg -ForegroundColor $(if ($exitCode -eq 0) { "Green" } else { "Red" })
    Add-Content -Path $logFile -Value $exitMsg

    if ($exitCode -ne 0) {
        $global:failedSteps++
        Write-Error "Step '$stepName' failed with exit code $exitCode"
    }
    return $exitCode
}

Log-Section "1. GIT METADATA & REPOSITORY STATUS"
Run-Step "Git Revision" "git rev-parse HEAD"
Run-Step "Git Status" "git status --porcelain"

Log-Section "2. IMMUTABLE FIXTURE CRYPTOGRAPHIC VERIFICATION"

# Expected fixture hashes and sizes - fail closed if any expected fixture is missing from disk.
$expectedFixtures = @{
    "v2_snapshot.bin"  = @{ Size = 859798; SHA256 = "425F6DB6459ABA70339062F53EE511D061F6DC0D0A005D3CD3D9F1E23ECDB09A" }
    "legacy_v0_wal.dat" = @{ Size = 1598;   SHA256 = "BBAF6EC70F724CE439C33859DD3283255A287C655A2AFD9D79DF6B568ED28C2C" }
}

$fixtureDir = "cmhm_core\tests\fixtures"

# FAIL CLOSED: Iterate over ALL expected fixture keys first to ensure no missing files!
foreach ($expectedName in $expectedFixtures.Keys) {
    $fixturePath = Join-Path $fixtureDir $expectedName
    if (-not (Test-Path $fixturePath)) {
        $global:failedSteps++
        $errMsg = "  FAIL CLOSED: Expected fixture '$expectedName' is MISSING from disk at '$fixturePath'!"
        Write-Error $errMsg
        Add-Content -Path $logFile -Value $errMsg
        continue
    }

    $item = Get-Item $fixturePath
    $hash = (Get-FileHash $item.FullName -Algorithm SHA256).Hash.ToUpper()
    $exp = $expectedFixtures[$expectedName]
    $msg = "Fixture: $expectedName`n  Path: $($item.FullName)`n  Size: $($item.Length) bytes`n  SHA256: $hash"
    Write-Host $msg -ForegroundColor Magenta
    Add-Content -Path $logFile -Value $msg

    if ($item.Length -ne $exp.Size) {
        $global:failedSteps++
        $errMsg = "  FAIL: Size mismatch for ${expectedName}: got $($item.Length) bytes, expected $($exp.Size)"
        Write-Error $errMsg
        Add-Content -Path $logFile -Value $errMsg
    } else {
        Add-Content -Path $logFile -Value "  OK: Size matches $($exp.Size) bytes"
    }

    if ($hash -ne $exp.SHA256) {
        $global:failedSteps++
        $errMsg = "  FAIL: SHA256 mismatch for ${expectedName}:`n    got:      $hash`n    expected: $($exp.SHA256)"
        Write-Error $errMsg
        Add-Content -Path $logFile -Value $errMsg
    } else {
        Add-Content -Path $logFile -Value "  OK: SHA256 matches $($exp.SHA256)"
    }
}

# Check for any unexpected binary fixtures on disk
$foundFiles = Get-ChildItem $fixtureDir -File | Where-Object { $_.Extension -ne ".md" }
foreach ($f in $foundFiles) {
    if (-not $expectedFixtures.ContainsKey($f.Name)) {
        $global:failedSteps++
        $errMsg = "  FAIL: Unexpected fixture $($f.Name) found on disk - no expected hash registered."
        Write-Error $errMsg
        Add-Content -Path $logFile -Value $errMsg
    }
}



Log-Section "3. FULL UNFILTERED WORKSPACE TEST BATTERY"
Run-Step "Workspace All Tests" "cargo test --workspace"

Log-Section "4. FIVE SEQUENTIAL E2E RUNS (--test-threads=1)"
for ($i = 1; $i -le 5; $i++) {
    Run-Step "Sequential E2E Run #$i" "cargo test -p cmhm_core --test retrieval_e2e -- --test-threads=1"
}

Log-Section "5. FIVE PARALLEL E2E RUNS (default parallelism)"
for ($i = 1; $i -le 5; $i++) {
    Run-Step "Parallel E2E Run #$i" "cargo test -p cmhm_core --test retrieval_e2e"
}

Log-Section "6. FIVE MCP TOOL LAYER RUNS"
for ($i = 1; $i -le 5; $i++) {
    Run-Step "MCP Tool Tests Run #$i" "cargo test -p cmhm_mcp --test tool_tests"
}

Log-Section "7. VERIFICATION BATTERY COMPLETE"
if ($global:failedSteps -gt 0) {
    $failMsg = "Verification battery FAILED with $global:failedSteps failed step(s). See $logFile for details."
    Write-Host $failMsg -ForegroundColor Red
    Add-Content -Path $logFile -Value "`n[OVERALL RESULT] FAILED ($global:failedSteps failures)`n"
    exit 1
} else {
    $completeMsg = "All steps executed with EXIT CODE 0. Comprehensive verification evidence saved to: $logFile"
    Write-Host $completeMsg -ForegroundColor Green
    Add-Content -Path $logFile -Value "`n[OVERALL RESULT] PASSED (0 failures)`n$completeMsg`n"
    exit 0
}

