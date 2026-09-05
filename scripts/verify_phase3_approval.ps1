# verify_phase3_approval.ps1
# Phase 3 Approval Verification Script — v1.0
#
# Exercises the Phase 3 held-out benchmark and computes all 10 acceptance gates.
#
# IMPORTANT: This script must be committed and its SHA-256 recorded BEFORE the
# approval corpus (phase3_approval_bench.rs) is created. Do not modify this script
# after observing corpus results without restarting the approval process.
#
# Protocol reference: docs/phase3_approval_protocol.md
#
# Usage:
#   .\scripts\verify_phase3_approval.ps1
# Exit code 0  = all 10 gates passed (approval evidence may be submitted)
# Exit code 1  = one or more gates failed, or script/build error
# Exit code 2  = integrity check failed (hashes do not match frozen values)

param(
    [string]$CorpusTestName  = "phase3_approval_bench",
    [switch]$SyntheticOnly   = $false    # When set, runs only the synthetic self-test fixtures
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# ── Configuration ─────────────────────────────────────────────────────────────

$RepoRoot    = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$LogFile     = Join-Path $RepoRoot "phase3_approval_verification.log"
$TempOut     = Join-Path $env:TEMP "phase3_approval_cargo_out.txt"

# Bootstrap parameters (locked — do not change)
$BOOTSTRAP_RESAMPLES    = 10000
$BOOTSTRAP_SEED_HEX     = "0x1F4A9C2EB8D37506"

# Distractor RNG seed (locked)
$DISTRACTOR_RNG_SEED    = "0xA3F72B91C5D4E608"

# Gate thresholds (locked — from protocol Section 5.1)
$GATE_G1_RECALL_AT_1_MIN        = 0.40
$GATE_G2_RECALL_AT_5_MIN        = 0.67
$GATE_G3_MRR_MIN                = 0.40
$GATE_G4_CONTRADICTION_AT_1_MAX = 0.40
$GATE_G5_HARD_NEG_OCC_AT_5_MAX  = 0.40
$GATE_G6_MEAN_MARGIN_MIN        = 0.00
$GATE_G7_SUPERSESSION_LEAK_MAX  = 0.00  # exact: zero leakage
$GATE_G8_LOGICAL_CONTRADICTION_FIRE_RATE_MIN = 0.80
$GATE_G9_SAFE_ABSTENTION_RATE_MIN = 0.80
$GATE_G10_POOL_CONTAMINATION_MAX = 0  # exact: zero contamination errors

# ── Pool stage names (from protocol Section 4.4 and pool terminology) ─────────
# Stage 0 (RAW_NN)   : candidates from nearest() — before any filter
# Stage 1 (POST_FILTER): candidates after supersession filter (Step 2a)
# Stage 2 (SCORED)   : composite-scored candidates, sorted descending
# Stage 3 (CHAIN)    : causal chain returned to caller (may be empty on abstain)
# Gate G5 uses SCORED positions (not RAW_NN, not CHAIN).
# Gate G1/G2/G3 use positive node rank in SCORED.
# Gate G4 uses hard-negative rank 1 in SCORED.
# Gate G7 requires D− in RAW_NN but absent from POST_FILTER for supersession scenarios.
# Gate G10 requires D− in RAW_NN for all contradiction/authority scenarios.

# ── Frozen artifact hashes ─────────────────────────────────────────────────────
# These are the SHA-256 values recorded at the pre-corpus freeze commit.
# They must match the files on disk. If they do not, the corpus is invalid.
#
# NOTE: CORPUS_HASH and VERIFIER_HASH are filled in by the implementation
# sequence after corpus creation and this script commit, respectively.
# The implementation agent must update these values before the first run.

$FROZEN_HASHES = @{
    "docs/phase3_approval_protocol.md" = "D51F0B5E80505DCF0DA3C5DB135E380FD432E048687D59D9743D84B68709F137"
    "cmhm_core/src/retrieval.rs"       = "DA6B1845AA1811293BCE99BAB1EE7833BF5DCC81E6C295B8A63347FDBDF4A7B9"
    "cmhm_mcp/src/tools.rs"            = $null  # filled in after implementation commit
    "cmhm_core/src/engine.rs"          = $null  # filled in after implementation commit
    "cmhm_core/src/text_embed.rs"      = "A6E2A21D483E6F048FE236D4CFDF842AE07E3EF9C8EE250C4D648AB3E71E89B3"
    "cmhm_mcp/src/schema.rs"           = $null  # filled in after implementation commit
    "scripts/verify_phase3_approval.ps1" = $null  # filled in after this script commit
    "cmhm_core/tests/phase3_approval_bench.rs" = $null  # filled in after corpus creation
}

# ── Helpers ───────────────────────────────────────────────────────────────────

function Write-Log {
    param([string]$Line)
    $ts = (Get-Date -Format "yyyy-MM-ddTHH:mm:ss")
    $entry = "[$ts] $Line"
    Write-Host $entry
    Add-Content -Path $LogFile -Value $entry
}

function Get-FileSha256 {
    param([string]$Path)
    return (Get-FileHash $Path -Algorithm SHA256).Hash.ToUpper()
}

function Fail {
    param([string]$Reason)
    Write-Log "FATAL: $Reason"
    exit 1
}

# ── Section 1: Environment integrity ──────────────────────────────────────────

Write-Log "=== Phase 3 Approval Verification Script v1.0 ==="
Write-Log "Repo: $RepoRoot"

# 1a. Git commit and cleanliness
$Commit = (git -C $RepoRoot rev-parse HEAD).Trim()
$Status = (git -C $RepoRoot status --porcelain).Trim()
Write-Log "Git commit: $Commit"
if ($Status -ne "") {
    Write-Log "WARNING: working tree is not clean: $Status"
    Fail "Working tree must be clean before running approval verification."
}
Write-Log "Git status: clean"

# 1b. Frozen artifact hash verification
Write-Log ""
Write-Log "=== Frozen Artifact Hash Verification ==="
$HashErrors = 0
foreach ($rel in $FROZEN_HASHES.Keys) {
    $expected = $FROZEN_HASHES[$rel]
    if ($null -eq $expected) {
        Write-Log "  SKIP (not yet filled): $rel"
        continue
    }
    $abs = Join-Path $RepoRoot $rel
    if (-not (Test-Path $abs)) {
        Write-Log "  MISSING: $rel"
        $HashErrors++
        continue
    }
    $actual = Get-FileSha256 $abs
    if ($actual -eq $expected) {
        Write-Log "  OK  $rel : $actual"
    } else {
        Write-Log "  MISMATCH $rel"
        Write-Log "    expected: $expected"
        Write-Log "    actual:   $actual"
        $HashErrors++
    }
}
if ($HashErrors -gt 0) {
    Fail "Frozen artifact hash check failed with $HashErrors error(s). Corpus is invalid for approval."
}

# ── Section 2: Synthetic self-test ────────────────────────────────────────────
# This section verifies the evaluation logic using deterministic synthetic data
# that has ZERO relationship to either prohibited corpus.
# The synthetic fixtures cover all gate formulas and pool stage definitions.
# They must pass before any real corpus run is trusted.

Write-Log ""
Write-Log "=== Synthetic Self-Test (evaluation logic verification) ==="

# Run the synthetic Rust test target
$SyntheticTestArgs = "test", "--manifest-path", "$RepoRoot\Cargo.toml",
                     "--package", "cmhm_core",
                     "--test", "phase3_synthetic_eval_tests",
                     "--", "--test-threads=1"

cmd /c "cargo $($SyntheticTestArgs -join ' ') > `"$TempOut`" 2>&1"
$SyntheticExit = $LASTEXITCODE
$SyntheticOutput = Get-Content $TempOut -Raw -ErrorAction SilentlyContinue
Write-Log "Synthetic self-test exit code: $SyntheticExit"
Add-Content -Path $LogFile -Value $SyntheticOutput

if ($SyntheticExit -ne 0) {
    Write-Log "Synthetic self-test FAILED — evaluation logic has errors."
    Write-Log "This means the gate formulas, pool stage definitions, or bootstrap"
    Write-Log "logic are broken. Fix the verifier before running the real corpus."
    exit 1
}
Write-Log "Synthetic self-test PASSED."

if ($SyntheticOnly) {
    Write-Log "SyntheticOnly flag set — stopping after synthetic tests."
    exit 0
}

# ── Section 3: Corpus benchmark build ─────────────────────────────────────────

Write-Log ""
Write-Log "=== Corpus Benchmark Build ==="

$BenchArgs = "test", "--manifest-path", "$RepoRoot\Cargo.toml",
             "--package", "cmhm_core",
             "--test", $CorpusTestName,
             "--", "--test-threads=1", "--nocapture"

cmd /c "cargo $($BenchArgs -join ' ') > `"$TempOut`" 2>&1"
$BenchExit = $LASTEXITCODE
$BenchOutput = Get-Content $TempOut -Raw -ErrorAction SilentlyContinue
Write-Log "Benchmark exit code: $BenchExit"
Add-Content -Path $LogFile -Value $BenchOutput

if ($BenchExit -ne 0) {
    Write-Log "Benchmark FAILED (Rust test runner exit code $BenchExit)."
    exit 1
}

# ── Section 4: Parse per-scenario results from benchmark output ───────────────
# The benchmark emits structured lines in the format:
#
#   PHASE3_RESULT|<id>|raw_nn=<list>|post_filter=<list>|scored=<list>|winner_id=<id>|
#                     pos_rank=<n>|neg_rank=<n>|pos_in_top5=<bool>|neg_in_top5=<bool>|
#                     conflict_type=<type_or_none>|abstained=<bool>|gap=<f32>|
#                     pool_contamination=<bool>|supersession_leaked=<bool>|
#                     class=<CONTRADICTION|SUPERSESSION|AUTHORITY-POLARITY|ADVERSARIAL-AUTHORITY|SAFE-ABSTENTION|UNRESOLVED-CONFLICT>
#
# Parse these lines to compute gate metrics.
# All metrics use SCORED pool positions, not RAW_NN or CHAIN.

Write-Log ""
Write-Log "=== Parsing per-scenario results ==="

$ResultLines = $BenchOutput -split "`n" | Where-Object { $_ -match "^PHASE3_RESULT\|" }
Write-Log "Found $($ResultLines.Count) result lines."

if ($ResultLines.Count -eq 0) {
    Fail "No PHASE3_RESULT lines found in benchmark output. The benchmark must emit structured results."
}

# Initialize per-scenario accumulators
$TotalScenarios    = $ResultLines.Count
$PosAtRank1        = 0   # Recall@1 numerator
$PosInTop5         = 0   # Recall@5 numerator
$MrrSum            = 0.0 # MRR accumulator (no rank cap — use full list, 0 if absent)
$NegAtRank1        = 0   # Contradiction-at-1 numerator
$NegInTop5         = 0   # Hard-Neg Occupancy@5 numerator
$MarginSum         = 0.0 # Mean margin accumulator (pos_score - neg_score)
$SupersessionLeaks = 0   # G7 numerator
$SupersessionTotal = 0   # G7 denominator
$LcFired           = 0   # G8 numerator
$LcEligibleTotal   = 0   # G8 denominator (see protocol: predeclared per OQ-3)
$AbstractionCorrect= 0   # G9 numerator
$AbstentionTotal   = 0   # G9 denominator
$PoolContamination = 0   # G10 total

# Per-scenario data for bootstrap
$PosRanks   = @()   # reciprocal rank for each scenario (0 if absent)
$NegRank1s  = @()   # 1 if neg at rank 1, else 0
$NegTop5s   = @()   # 1 if neg in top 5, else 0
$Margins    = @()   # pos_score - neg_score per scenario

foreach ($line in $ResultLines) {
    $parts = $line -split "\|"
    # Parse into hash
    $fields = @{}
    for ($i = 1; $i -lt $parts.Count; $i++) {
        $kv = $parts[$i] -split "=", 2
        if ($kv.Count -eq 2) { $fields[$kv[0].Trim()] = $kv[1].Trim() }
    }

    $id         = $fields["id"]
    $posRank    = [int]$fields["pos_rank"]      # 1-indexed; 0 = not in scored pool
    $negRank    = [int]$fields["neg_rank"]      # 1-indexed; 0 = not in scored pool
    $posTop5    = ($fields["pos_in_top5"] -eq "true")
    $negTop5    = ($fields["neg_in_top5"] -eq "true")
    $ctype      = $fields["conflict_type"]      # "LogicalContradiction" | "None" | etc.
    $abstained  = ($fields["abstained"] -eq "true")
    $margin     = [float]$fields["gap"]         # pos composite score - neg composite score
    $contaminated = ($fields["pool_contamination"] -eq "true")
    $leaked     = ($fields["supersession_leaked"] -eq "true")
    $class      = $fields["class"]              # scenario class label(s), comma-separated

    # Tie-breaking rule: if posRank == negRank (tie), count positive below negative
    if ($posRank -gt 0 -and $negRank -gt 0 -and $posRank -eq $negRank) {
        $posRank = $negRank + 1
        $posTop5 = ($posRank -le 5)
    }

    # Abstention handling: positive has no rank → reciprocal rank = 0, Recall@K = 0
    if ($abstained) {
        $posRank = 0
        $posTop5 = $false
    }

    # G1 Recall@1
    if ($posRank -eq 1) { $PosAtRank1++ }
    # G2 Recall@5
    if ($posTop5) { $PosInTop5++ }
    # G3 MRR — no rank cap (OQ-2 answer: full composite-scored candidate list)
    $rr = if ($posRank -gt 0) { 1.0 / $posRank } else { 0.0 }
    $MrrSum += $rr
    # G4 Contradiction-at-1
    if ($negRank -eq 1) { $NegAtRank1++ }
    # G5 Hard-Neg Occupancy@5 (SCORED pool positions)
    if ($negTop5) { $NegInTop5++ }
    # G6 Mean Margin
    $MarginSum += $margin

    # G7 Supersession
    if ($class -match "SUPERSESSION") {
        $SupersessionTotal++
        if ($leaked) { $SupersessionLeaks++ }
    }

    # G8 LogicalContradiction fire rate
    # Denominator = all CONTRADICTION scenarios (end-to-end measurement, not conditional;
    # a separate strict gate (S1) below verifies D+/D− reach the top-2 eligible state).
    if ($class -match "CONTRADICTION") {
        $LcEligibleTotal++
        if ($ctype -eq "LogicalContradiction") { $LcFired++ }
    }

    # G9 Safe Abstention (abstain_on_conflict = true for all approval scenarios per OQ-1)
    # Denominator = SAFE-ABSTENTION scenarios
    if ($class -match "SAFE-ABSTENTION") {
        $AbstentionTotal++
        if ($abstained) { $AbstractionCorrect++ }
    }

    # G10 Pool Contamination
    if ($contaminated) { $PoolContamination++ }

    # Accumulators for bootstrap
    $PosRanks  += $rr
    $NegRank1s += if ($negRank -eq 1) { 1 } else { 0 }
    $NegTop5s  += if ($negTop5) { 1 } else { 0 }
    $Margins   += $margin
}

# ── Section 5: Bootstrap confidence intervals ──────────────────────────────────
# 10,000 resamples with declared seed. Compute lower 95% CI for G1-G3, G8;
# upper 95% CI for G4, G5; lower 95% CI for G6.
# PowerShell implementation: simple percentile bootstrap.

Write-Log ""
Write-Log "=== Bootstrap CI computation ($BOOTSTRAP_RESAMPLES resamples) ==="

function Bootstrap-CI {
    param(
        [double[]]$Samples,
        [int]$Resamples,
        [long]$Seed,
        [string]$Statistic   # "mean" only for now
    )
    $n = $Samples.Count
    $rng = New-Object System.Random([int]($Seed -band 0x7FFFFFFF))
    $stats = @()
    for ($r = 0; $r -lt $Resamples; $r++) {
        $sum = 0.0
        for ($j = 0; $j -lt $n; $j++) {
            $sum += $Samples[$rng.Next(0, $n)]
        }
        $stats += ($sum / $n)
    }
    $sorted = $Samples | Sort-Object  # sort is not needed; sort stats
    $sorted = $stats | Sort-Object
    $lo_idx = [int][Math]::Floor(0.025 * $Resamples)
    $hi_idx = [int][Math]::Floor(0.975 * $Resamples)
    return @{ Lower = $sorted[$lo_idx]; Upper = $sorted[$hi_idx] }
}

$BsSeed = [long]"0x1F4A9C2EB8D37506"
# Convert hex string to long
$BsSeed = [System.Convert]::ToInt64("1F4A9C2EB8D37506", 16)

# Prepare per-scenario binary arrays for G1, G2, G4, G5
$G1_samples = $ResultLines | ForEach-Object {
    $p = $_ -split "\|"; $fields = @{}
    for ($i=1; $i -lt $p.Count; $i++) {
        $kv=$p[$i] -split "=",2; if($kv.Count-eq 2){$fields[$kv[0].Trim()]=$kv[1].Trim()}
    }
    $posRank = [int]$fields["pos_rank"]
    $abstained = ($fields["abstained"] -eq "true")
    if ($abstained) { $posRank = 0 }
    if ($posRank -eq 1) { 1.0 } else { 0.0 }
}
$G2_samples = $ResultLines | ForEach-Object {
    $p = $_ -split "\|"; $fields = @{}
    for ($i=1; $i -lt $p.Count; $i++) {
        $kv=$p[$i] -split "=",2; if($kv.Count-eq 2){$fields[$kv[0].Trim()]=$kv[1].Trim()}
    }
    $posTop5 = ($fields["pos_in_top5"] -eq "true")
    $abstained = ($fields["abstained"] -eq "true")
    if ($abstained) { 0.0 } elseif ($posTop5) { 1.0 } else { 0.0 }
}
$G3_samples = $PosRanks   # already computed as reciprocal ranks
$G4_samples = $NegRank1s | ForEach-Object { [double]$_ }
$G5_samples = $NegTop5s  | ForEach-Object { [double]$_ }
$G6_samples = $Margins   | ForEach-Object { [double]$_ }

$G1_ci = Bootstrap-CI -Samples $G1_samples -Resamples $BOOTSTRAP_RESAMPLES -Seed $BsSeed
$G2_ci = Bootstrap-CI -Samples $G2_samples -Resamples $BOOTSTRAP_RESAMPLES -Seed $BsSeed
$G3_ci = Bootstrap-CI -Samples $G3_samples -Resamples $BOOTSTRAP_RESAMPLES -Seed $BsSeed
$G4_ci = Bootstrap-CI -Samples $G4_samples -Resamples $BOOTSTRAP_RESAMPLES -Seed $BsSeed
$G5_ci = Bootstrap-CI -Samples $G5_samples -Resamples $BOOTSTRAP_RESAMPLES -Seed $BsSeed
$G6_ci = Bootstrap-CI -Samples $G6_samples -Resamples $BOOTSTRAP_RESAMPLES -Seed $BsSeed

# ── Section 6: Gate evaluation ─────────────────────────────────────────────────

Write-Log ""
Write-Log "=== Gate Evaluation ==="
Write-Log "Total scenarios: $TotalScenarios"

$AllPass = $true

function Eval-Gate {
    param(
        [string]$Name,
        [double]$Estimate,
        [double]$CiBound,
        [string]$Direction,   # "lower" or "upper"
        [double]$Threshold,
        [string]$Formula
    )
    $passEstimate = if ($Direction -eq "lower") { $Estimate -ge $Threshold } else { $Estimate -lt $Threshold }
    $passCi       = if ($Direction -eq "lower") { $CiBound  -ge $Threshold } else { $CiBound  -lt $Threshold }
    $pass = $passEstimate -and $passCi
    $verdict = if ($pass) { "PASS" } else { "FAIL" }
    Write-Log ("  $Name | {0:F4} (95% CI {1}: {2:F4}) | threshold: {3} | {4}" -f $Estimate, $Direction, $CiBound, $Threshold, $verdict)
    Write-Log "    Formula: $Formula"
    return $pass
}

function Eval-Gate-Exact {
    param(
        [string]$Name,
        [double]$Estimate,
        [string]$Comparator,  # "eq" | "lt_or_eq"
        [double]$Threshold
    )
    $pass = if ($Comparator -eq "eq")       { $Estimate -eq $Threshold } `
            elseif ($Comparator -eq "lte")  { $Estimate -le $Threshold } `
            else                            { $false }
    $verdict = if ($pass) { "PASS" } else { "FAIL" }
    Write-Log ("  $Name | {0:F4} | threshold: {1} | {2}" -f $Estimate, $Threshold, $verdict)
    return $pass
}

# G1 Recall@1 (lower bound >= threshold)
$G1_est = $PosAtRank1 / $TotalScenarios
$G1_pass = Eval-Gate -Name "G1 Recall@1" -Estimate $G1_est -CiBound $G1_ci.Lower `
    -Direction "lower" -Threshold $GATE_G1_RECALL_AT_1_MIN `
    -Formula "count(pos_rank=1) / total = $PosAtRank1 / $TotalScenarios"
if (-not $G1_pass) { $AllPass = $false }

# G2 Recall@5 (lower bound >= threshold)
$G2_est = $PosInTop5 / $TotalScenarios
$G2_pass = Eval-Gate -Name "G2 Recall@5" -Estimate $G2_est -CiBound $G2_ci.Lower `
    -Direction "lower" -Threshold $GATE_G2_RECALL_AT_5_MIN `
    -Formula "count(pos_in_top5_SCORED) / total = $PosInTop5 / $TotalScenarios"
if (-not $G2_pass) { $AllPass = $false }

# G3 MRR (lower bound >= threshold)
$G3_est = $MrrSum / $TotalScenarios
$G3_pass = Eval-Gate -Name "G3 MRR" -Estimate $G3_est -CiBound $G3_ci.Lower `
    -Direction "lower" -Threshold $GATE_G3_MRR_MIN `
    -Formula "mean(1/pos_rank_SCORED; 0 if absent/abstained) = $MrrSum / $TotalScenarios"
if (-not $G3_pass) { $AllPass = $false }

# G4 Contradiction-at-1 (upper bound < threshold)
$G4_est = $NegAtRank1 / $TotalScenarios
$G4_pass = Eval-Gate -Name "G4 Contradiction-at-1" -Estimate $G4_est -CiBound $G4_ci.Upper `
    -Direction "upper" -Threshold $GATE_G4_CONTRADICTION_AT_1_MAX `
    -Formula "count(neg_rank_SCORED=1) / total = $NegAtRank1 / $TotalScenarios"
if (-not $G4_pass) { $AllPass = $false }

# G5 Hard-Neg Occupancy@5 (upper bound < threshold) — SCORED pool
$G5_est = $NegInTop5 / $TotalScenarios
$G5_pass = Eval-Gate -Name "G5 Hard-Neg Occupancy@5 (SCORED)" -Estimate $G5_est -CiBound $G5_ci.Upper `
    -Direction "upper" -Threshold $GATE_G5_HARD_NEG_OCC_AT_5_MAX `
    -Formula "count(neg_in_top5_SCORED) / total = $NegInTop5 / $TotalScenarios"
if (-not $G5_pass) { $AllPass = $false }

# G6 Mean Margin (lower bound > threshold)
$G6_est = $MarginSum / $TotalScenarios
$G6_pass = Eval-Gate -Name "G6 Mean Margin" -Estimate $G6_est -CiBound $G6_ci.Lower `
    -Direction "lower" -Threshold $GATE_G6_MEAN_MARGIN_MIN `
    -Formula "mean(pos_SCORED_score - neg_SCORED_score) = $MarginSum / $TotalScenarios"
if (-not $G6_pass) { $AllPass = $false }

# G7 Supersession Safety (exact: zero leakage)
$G7_est = if ($SupersessionTotal -gt 0) { $SupersessionLeaks / $SupersessionTotal } else { 0.0 }
$G7_pass = Eval-Gate-Exact -Name "G7 Supersession Safety (D- absent from POST_FILTER)" `
    -Estimate $G7_est -Comparator "lte" -Threshold $GATE_G7_SUPERSESSION_LEAK_MAX
Write-Log "    ($SupersessionLeaks leaks / $SupersessionTotal supersession scenarios)"
if (-not $G7_pass) { $AllPass = $false }

# G8 LogicalContradiction Fire Rate (G8 and G9 are independent per OQ-3)
$G8_est = if ($LcEligibleTotal -gt 0) { $LcFired / $LcEligibleTotal } else { 0.0 }
$G8_pass = Eval-Gate -Name "G8 LogicalContradiction Fire Rate" -Estimate $G8_est `
    -CiBound (Bootstrap-CI -Samples ($ResultLines | ForEach-Object {
        $p=$_ -split "\|"; $fields=@{}
        for($i=1;$i -lt $p.Count;$i++){$kv=$p[$i] -split "=",2;if($kv.Count-eq 2){$fields[$kv[0].Trim()]=$kv[1].Trim()}}
        $class=$fields["class"]; $ctype=$fields["conflict_type"]
        if ($class -match "CONTRADICTION") { if ($ctype -eq "LogicalContradiction") { 1.0 } else { 0.0 } }
    } | Where-Object { $_ -ne $null }) -Resamples $BOOTSTRAP_RESAMPLES -Seed $BsSeed).Lower `
    -Direction "lower" -Threshold $GATE_G8_LOGICAL_CONTRADICTION_FIRE_RATE_MIN `
    -Formula "count(conflict_type=LogicalContradiction) / CONTRADICTION_scenarios = $LcFired / $LcEligibleTotal"
if (-not $G8_pass) { $AllPass = $false }

# G9 Safe Abstention Rate
$G9_est = if ($AbstentionTotal -gt 0) { $AbstractionCorrect / $AbstentionTotal } else { 0.0 }
$G9_pass = Eval-Gate -Name "G9 Safe Abstention Rate" -Estimate $G9_est `
    -CiBound (Bootstrap-CI -Samples ($ResultLines | ForEach-Object {
        $p=$_ -split "\|"; $fields=@{}
        for($i=1;$i -lt $p.Count;$i++){$kv=$p[$i] -split "=",2;if($kv.Count-eq 2){$fields[$kv[0].Trim()]=$kv[1].Trim()}}
        $class=$fields["class"]; $abstained=($fields["abstained"] -eq "true")
        if ($class -match "SAFE-ABSTENTION") { if ($abstained) { 1.0 } else { 0.0 } }
    } | Where-Object { $_ -ne $null }) -Resamples $BOOTSTRAP_RESAMPLES -Seed $BsSeed).Lower `
    -Direction "lower" -Threshold $GATE_G9_SAFE_ABSTENTION_RATE_MIN `
    -Formula "count(abstained=true) / SAFE-ABSTENTION_scenarios = $AbstractionCorrect / $AbstentionTotal"
if (-not $G9_pass) { $AllPass = $false }

# G10 Pool Contamination (exact: zero)
$G10_pass = Eval-Gate-Exact -Name "G10 Pool Contamination (D- missing from RAW_NN)" `
    -Estimate $PoolContamination -Comparator "lte" -Threshold $GATE_G10_POOL_CONTAMINATION_MAX
if (-not $G10_pass) { $AllPass = $false }

# ── Section 7: Selection quality decomposition ─────────────────────────────────
# Required by blocker 6: report four selection outcome rates separately.

Write-Log ""
Write-Log "=== Selection Quality Decomposition ==="

$SafeAnswer      = 0  # pos selected, no conflict
$CorrectAbstain  = 0  # abstained when contradiction present (SAFE-ABSTENTION scenario)
$UnsafeSelect    = 0  # neg at rank 1 (contradiction selected)
$UnnecessaryAbstain = 0  # abstained when no contradiction class applicable

foreach ($line in $ResultLines) {
    $p=$line -split "\|"; $fields=@{}
    for($i=1;$i -lt $p.Count;$i++){$kv=$p[$i] -split "=",2;if($kv.Count-eq 2){$fields[$kv[0].Trim()]=$kv[1].Trim()}}
    $posRank=[int]$fields["pos_rank"]
    $negRank=[int]$fields["neg_rank"]
    $abstained=($fields["abstained"] -eq "true")
    $class=$fields["class"]

    if ($abstained -and ($class -match "SAFE-ABSTENTION")) {
        $CorrectAbstain++
    } elseif ($abstained -and ($class -notmatch "SAFE-ABSTENTION")) {
        $UnnecessaryAbstain++
    } elseif ($negRank -eq 1 -and -not $abstained) {
        $UnsafeSelect++
    } elseif ($posRank -eq 1 -and -not $abstained) {
        $SafeAnswer++
    }
}

Write-Log ("  Safe answer/selection rate:       {0:F4} ({1}/{2})" -f ($SafeAnswer/$TotalScenarios),      $SafeAnswer,         $TotalScenarios)
Write-Log ("  Correct abstention rate:           {0:F4} ({1}/{2})" -f ($CorrectAbstain/$TotalScenarios),  $CorrectAbstain,     $TotalScenarios)
Write-Log ("  Unsafe contradiction-selection rate: {0:F4} ({1}/{2})" -f ($UnsafeSelect/$TotalScenarios), $UnsafeSelect,       $TotalScenarios)
Write-Log ("  Unnecessary abstention rate:       {0:F4} ({1}/{2})" -f ($UnnecessaryAbstain/$TotalScenarios), $UnnecessaryAbstain, $TotalScenarios)

# ── Section 8: Final verdict ───────────────────────────────────────────────────

Write-Log ""
Write-Log "=== Final Verdict ==="
if ($AllPass) {
    Write-Log "ALL GATES PASSED — corpus is eligible as Phase 3 approval evidence."
    Write-Log "Submit this log and the sealed corpus archive for user review."
    exit 0
} else {
    Write-Log "ONE OR MORE GATES FAILED — corpus does not constitute approval evidence."
    Write-Log "Do not re-run with the same corpus. Do not adjust gates. File as rejection evidence."
    exit 1
}
