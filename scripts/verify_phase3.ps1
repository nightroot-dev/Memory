# ==============================================================================
# verify_phase3.ps1 - Phase 3 Held-Out Retrieval Benchmark Verification Driver
# ==============================================================================
# Strict fail-closed verification driver for Phase 3 operational retrieval safety.
# Records complete execution evidence, git status, and metrics to phase3_verification_evidence.log.
# ==============================================================================

$ErrorActionPreference = "Stop"
$logFile = "phase3_verification_evidence.log"

if (Test-Path $logFile) {
    Remove-Item $logFile -Force
}

function Log-Output([string]$msg) {
    Write-Host $msg
    Add-Content -Path $logFile -Value $msg -Encoding utf8
}

function Log-Section([string]$title) {
    $line = "=" * 80
    Log-Output ""
    Log-Output $line
    Log-Output "[SECTION] $title"
    Log-Output ("Timestamp: " + [System.DateTime]::UtcNow.ToString("yyyy-MM-dd HH:mm:ss") + " UTC")
    Log-Output $line
    Log-Output ""
}

$global:failedSteps = 0

function Run-Step([string]$stepName, [string]$command) {
    $subline = "-" * 80
    Log-Output ""
    Log-Output $subline
    Log-Output "[STEP] $stepName"
    Log-Output "Command: $command"
    Log-Output $subline
    Log-Output ""

    try {
        $pinfo = New-Object System.Diagnostics.ProcessStartInfo
        $pinfo.FileName = "cmd.exe"
        $pinfo.Arguments = "/c $command"
        $pinfo.RedirectStandardOutput = $true
        $pinfo.RedirectStandardError = $true
        $pinfo.UseShellExecute = $false
        $pinfo.CreateNoWindow = $true

        $proc = New-Object System.Diagnostics.Process
        $proc.StartInfo = $pinfo
        $proc.Start() | Out-Null

        $stdout = $proc.StandardOutput.ReadToEnd()
        $stderr = $proc.StandardError.ReadToEnd()
        $proc.WaitForExit()

        $exitCode = $proc.ExitCode

        if ($stdout.Length -gt 0) {
            Log-Output $stdout
        }
        if ($stderr.Length -gt 0) {
            Log-Output $stderr
        }

        Log-Output ""
        Log-Output "[RESULT] Exit Code: $exitCode"

        if ($exitCode -ne 0) {
            $global:failedSteps++
            Log-Output "[FAIL] Step '$stepName' exited with code $exitCode"
        }
    } catch {
        $global:failedSteps++
        Log-Output "[ERROR] Failed to execute step '$stepName': $_"
    }
}

# ─── Section 1: Git Metadata & Status ─────────────────────────────────────────
Log-Section "1. GIT METADATA & REPOSITORY STATUS"
Run-Step "Git Revision" "git rev-parse HEAD"
Run-Step "Git Status" "git status --porcelain"

# ─── Section 2: Full Workspace Integrity Check ────────────────────────────────
Log-Section "2. FULL WORKSPACE TEST SUITE (REGRESSION CHECK)"
Run-Step "Workspace All Tests" "cargo test --workspace"

# ─── Section 3: Phase 3 Held-Out Benchmark (3 Deterministic Runs) ─────────────
Log-Section "3. PHASE 3 HELD-OUT RETRIEVAL BENCHMARK"
Run-Step "Phase 3 Operational Benchmark Run #1" "cargo test -p cmhm_core --test phase3_held_out_bench -- --nocapture"
Run-Step "Phase 3 Operational Benchmark Run #2" "cargo test -p cmhm_core --test phase3_held_out_bench -- --nocapture"
Run-Step "Phase 3 Operational Benchmark Run #3" "cargo test -p cmhm_core --test phase3_held_out_bench -- --nocapture"

# ─── Section 4: Summary and Exit ──────────────────────────────────────────────
Log-Section "4. PHASE 3 VERIFICATION SUMMARY"
if ($global:failedSteps -eq 0) {
    Log-Output "[OVERALL RESULT] PASSED (0 failures)"
    Log-Output "All Phase 3 locked acceptance gates and regression tests passed with Exit Code 0."
    Log-Output "Evidence saved to: $logFile"
    exit 0
} else {
    Log-Output "[OVERALL RESULT] FAILED ($global:failedSteps failures)"
    Log-Output "Phase 3 verification failed. Inspect $logFile for details."
    exit 1
}
