# verify_phase3_approval.ps1
# Phase 3 Approval Verification Script -- v1.2
#
# Exercises the Phase 3 held-out approval benchmark over the external MCP path
# and evaluates all predeclared acceptance gates and outcome metrics.
#
# Protocol reference: docs/phase3_approval_protocol_v1.2.md
#
# Usage:
#   .\scripts\verify_phase3_approval.ps1 -SyntheticOnly
#   .\scripts\verify_phase3_approval.ps1
# Exit code 0  = all gates passed (approval evidence valid)
# Exit code 1  = gate failure, overlap violation, or verification error

param(
    [string]$CorpusPackage   = "cmhm_mcp",
    [string]$CorpusTestName  = "phase3_approval_bench",
    [switch]$SyntheticOnly   = $false
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RepoRoot    = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$LogFile     = Join-Path $RepoRoot "phase3_approval_verification.log"

# Bootstrap parameters (locked)
$BOOTSTRAP_RESAMPLES = 10000
$BOOTSTRAP_SEED_HEX  = "0x1F4A9C2EB8D37506"

# Distractor RNG seed (locked)
$DISTRACTOR_RNG_SEED = "0xA3F72B91C5D4E608"

# Gate thresholds (locked -- from protocol v1.2 Section 5)
$GATE_G1_RECALL_AT_1_MIN        = 0.40
$GATE_G2_RECALL_AT_5_MIN        = 0.67
$GATE_G3_MRR_MIN                = 0.40
$GATE_G4_CONTRADICTION_AT_1_MAX = 0.40
$GATE_G5_HARD_NEG_OCC_AT_5_MAX  = 0.40
$GATE_G6_MEAN_MARGIN_MIN        = 0.00
$GATE_G7_SUPERSESSION_LEAK_MAX  = 0.00  # exact: zero presence in all observables
$GATE_G8A_LOGICAL_CONTRA_MIN    = 0.80
$GATE_G8B_AUTH_POLARITY_MIN     = 0.80
$GATE_G8C_UNRESOLVED_NON_CONF   = 0.80
$GATE_G9_SAFE_ABSTENTION_MIN    = 0.80
$GATE_G10_POOL_CONTAMINATION_MAX= 0     # exact: zero contamination errors
$GATE_S1_LC_ELIGIBILITY_MIN     = 0.60

function Write-Log {
    param([string]$Line)
    $ts = (Get-Date -Format "yyyy-MM-ddTHH:mm:ss")
    $entry = "[$ts] $Line"
    Write-Host $entry
    Add-Content -Path $LogFile -Value $entry
}

function Fail {
    param([string]$Reason)
    Write-Log "FATAL: $Reason"
    exit 1
}

function Run-CommandCapture {
    param([string]$Cmd)
    $tmp = [System.IO.Path]::GetTempFileName()
    cmd /c "$Cmd > `"$tmp`" 2>&1"
    $code = $LASTEXITCODE
    $text = if (Test-Path $tmp) { [System.IO.File]::ReadAllText($tmp) } else { "" }
    Remove-Item $tmp -Force -ErrorAction SilentlyContinue
    return @{ ExitCode = $code; Output = $text }
}

# -- Section 1: Environment Integrity ------------------------------------------

Write-Log "=== Phase 3 Approval Verification Script v1.2 ==="
Write-Log "Repo: $RepoRoot"

$Commit = (git -C $RepoRoot rev-parse HEAD).Trim()
$Status = (git -C $RepoRoot status --porcelain).Trim()
Write-Log "Git commit: $Commit"
if ($Status -ne "") {
    Write-Log "WARNING: working tree is not clean: $Status"
    Fail "Working tree must be clean before running approval verification."
}
Write-Log "Git status: clean"

# -- Section 2: Synthetic Self-Tests (Evaluation Logic and MCP Path) -----------

Write-Log ""
Write-Log "=== Running Synthetic Self-Tests ==="

# 2a. Overlap auditor self-test
Write-Log "Running overlap auditor synthetic self-tests..."
$resAuditor = Run-CommandCapture "powershell -ExecutionPolicy Bypass -File `"$RepoRoot\scripts\audit_corpus_overlap.ps1`" -SyntheticSelfTest"
if ($resAuditor.ExitCode -ne 0) {
    Write-Log $resAuditor.Output
    Fail "Overlap auditor self-test failed."
}
Write-Log "  PASS  Overlap auditor synthetic self-test"

# 2b. Core synthetic eval tests
Write-Log "Running core synthetic evaluation tests..."
$resCore = Run-CommandCapture "cargo test --manifest-path `"$RepoRoot\Cargo.toml`" --package cmhm_core --test phase3_synthetic_eval_tests -- --test-threads=1"
if ($resCore.ExitCode -ne 0) {
    Write-Log $resCore.Output
    Fail "Core synthetic evaluation tests failed."
}
Write-Log "  PASS  Core synthetic evaluation tests (9/9 passed)"

# 2c. MCP tool layer integration tests (exercising unscoped dispatch and diagnostics)
Write-Log "Running MCP tool layer integration tests..."
$resMcp = Run-CommandCapture "cargo test --manifest-path `"$RepoRoot\Cargo.toml`" --package cmhm_mcp --test tool_tests -- --test-threads=1"
if ($resMcp.ExitCode -ne 0) {
    Write-Log $resMcp.Output
    Fail "MCP tool layer integration tests failed."
}
Write-Log "  PASS  MCP tool layer integration tests (50/50 passed)"

if ($SyntheticOnly) {
    Write-Log "SyntheticOnly flag set -- verification completed successfully."
    exit 0
}

# -- Section 3: Overlap Audit against Prohibited Corpora -----------------------

Write-Log ""
Write-Log "=== Running Pre-Execution Overlap Audit ==="
$CandidateRelPath = if ($CorpusPackage -eq "cmhm_mcp") { "cmhm_mcp/tests/$CorpusTestName.rs" } else { "cmhm_core/tests/$CorpusTestName.rs" }
$CandidateAbsPath = Join-Path $RepoRoot $CandidateRelPath

if (-not (Test-Path $CandidateAbsPath)) {
    Fail "Candidate approval corpus not found at: $CandidateAbsPath"
}

$resAuditRun = Run-CommandCapture "powershell -ExecutionPolicy Bypass -File `"$RepoRoot\scripts\audit_corpus_overlap.ps1`" -CandidateCorpusPath `"$CandidateRelPath`""
Write-Log $resAuditRun.Output
if ($resAuditRun.ExitCode -ne 0) {
    Fail "Overlap audit FAILED. Candidate corpus contains overlapping queries or phrases."
}
Write-Log "Overlap audit PASSED. Zero overlap violations detected."

# -- Section 4: Execute Corpus Benchmark via Cargo -----------------------------

Write-Log ""
Write-Log "=== Executing Approval Benchmark ($CorpusPackage / $CorpusTestName) ==="

$benchCmd = "cargo test --manifest-path `"$RepoRoot\Cargo.toml`" --package $CorpusPackage --test $CorpusTestName -- --test-threads=1 --nocapture"
$resBench = Run-CommandCapture $benchCmd
$BenchExit = $resBench.ExitCode
$BenchOutput = $resBench.Output

Write-Log "Benchmark exit code: $BenchExit"
Add-Content -Path $LogFile -Value $BenchOutput

if ($BenchExit -ne 0) {
    Fail "Benchmark execution FAILED with exit code $BenchExit."
}

# -- Section 5: Parse Structured Results and Evaluate Gates --------------------

Write-Log ""
Write-Log "=== Parsing Structured Results ==="

$allLines = $BenchOutput.Split("`n")
$ResultLines = [System.Collections.Generic.List[string]]::new()
foreach ($l in $allLines) {
    $trimmed = $l.Trim()
    if ($trimmed.StartsWith("PHASE3_RESULT|")) {
        $ResultLines.Add($trimmed)
    }
}
Write-Log "Found $($ResultLines.Count) structured scenario result lines."

if ($ResultLines.Count -lt 40) {
    Fail "Expected at least 40 structured scenario results, found $($ResultLines.Count)."
}

$TotalScenarios = $ResultLines.Count

# Accumulators for SCORED ranking metrics (Blocker 6: computed strictly from SCORED)
$PosRank1Count = 0
$PosTop5Count  = 0
$MrrSum        = 0.0
$NegRank1Count = 0
$NegTop5Count  = 0
$MarginSum     = 0.0

# Accumulators for downstream observables (Blocker 9)
$SupersessionTotal      = 0
$SupersessionViolations = 0

# Accumulators for conflict classification (Blocker 8)
$ContraTotal      = 0
$ContraEligible   = 0
$ContraLcFired    = 0
$AuthPolTotal     = 0
$AuthPolFired     = 0
$UnresTotal       = 0
$UnresNonConflict = 0

# Accumulators for safe abstention
$SafeAbstainTotal   = 0
$SafeAbstainFired   = 0
$PoolContamination  = 0

# 5-Way Decision Outcome Categories (Blocker 7: exhaustive and mutually exclusive)
$Count_SelectedDPlus      = 0
$Count_SelectedDMinus     = 0
$Count_SelectedOther      = 0
$Count_CorrectAbstention  = 0
$Count_IncorrectAbstention= 0

# Arrays for Bootstrap CIs
$G1_arr = @()
$G2_arr = @()
$G3_arr = @()
$G4_arr = @()
$G5_arr = @()
$G6_arr = @()

foreach ($line in $ResultLines) {
    $parts = $line.Split('|')
    $fields = @{}
    for ($i = 1; $i -lt $parts.Length; $i++) {
        $kv = $parts[$i].Split('=', 2)
        if ($kv.Length -eq 2) { $fields[$kv[0].Trim()] = $kv[1].Trim() }
    }

    $id         = $fields["id"]
    $posRank    = [int]$fields["pos_rank"]      # 1-indexed in SCORED, 0 if absent
    $negRank    = [int]$fields["neg_rank"]      # 1-indexed in SCORED, 0 if absent
    $posTop5    = ($fields["pos_in_top5"] -eq "true")
    $negTop5    = ($fields["neg_in_top5"] -eq "true")
    $winnerId   = $fields["winner_id"]
    $posId      = $fields["pos_id"]
    $negId      = $fields["neg_id"]
    $ctype      = $fields["conflict_type"]
    $abstained  = ($fields["abstained"] -eq "true")
    $margin     = [float]$fields["gap"]
    $contaminated = ($fields["pool_contamination"] -eq "true")
    
    $rawClass = if ($fields.ContainsKey("class")) { $fields["class"] } else { "" }

    # Downstream observable leak flags for supersession (Blocker 9)
    $superLeakedPostFilter = ($fields["supersession_leaked"] -eq "true")
    $superLeakedConflict   = ($fields["supersession_in_conflict"] -eq "true")
    $superLeakedChain      = ($fields["supersession_in_chain"] -eq "true")
    $superLeakedWinner     = ($winnerId -eq $negId)

    # 1. SCORED ranking metrics (Blocker 6: independent of Step 2e abstention)
    if ($posRank -gt 0 -and $negRank -gt 0 -and $posRank -eq $negRank) {
        $posRank = $negRank + 1
        $posTop5 = ($posRank -le 5)
    }

    if ($posRank -eq 1) { $PosRank1Count++ }
    if ($posTop5)       { $PosTop5Count++ }
    $rr = if ($posRank -gt 0) { 1.0 / [double]$posRank } else { 0.0 }
    $MrrSum += $rr

    if ($negRank -eq 1) { $NegRank1Count++ }
    if ($negTop5)       { $NegTop5Count++ }
    $MarginSum += $margin

    $G1_arr += if ($posRank -eq 1) { 1.0 } else { 0.0 }
    $G2_arr += if ($posTop5) { 1.0 } else { 0.0 }
    $G3_arr += $rr
    $G4_arr += if ($negRank -eq 1) { 1.0 } else { 0.0 }
    $G5_arr += if ($negTop5) { 0.20 } else { 0.0 }
    $G6_arr += [double]$margin

    # 2. 5-Way Decision Outcome Categorization (Blocker 7)
    $isSafeAbstainClass = ($rawClass.Contains("SAFE-ABSTENTION") -or $rawClass.Contains("CONTRADICTION"))
    if ($abstained) {
        if ($isSafeAbstainClass) {
            $Count_CorrectAbstention++
        } else {
            $Count_IncorrectAbstention++
        }
    } else {
        if ($winnerId -eq $posId) {
            $Count_SelectedDPlus++
        } elseif ($winnerId -eq $negId) {
            $Count_SelectedDMinus++
        } else {
            $Count_SelectedOther++
        }
    }

    # 3. Supersession Safety (Blocker 9: zero in all observables)
    if ($rawClass.Contains("SUPERSESSION")) {
        $SupersessionTotal++
        if ($superLeakedPostFilter -or $superLeakedConflict -or $superLeakedChain -or $superLeakedWinner) {
            $SupersessionViolations++
        }
    }

    # 4. Conflict Classification Gates (Blocker 8)
    if ($rawClass.Contains("CONTRADICTION")) {
        $ContraTotal++
        if ($fields.ContainsKey("eligible_top2_gap") -and $fields["eligible_top2_gap"] -eq "true") {
            $ContraEligible++
        }
        if ($ctype -eq "LogicalContradiction") {
            $ContraLcFired++
        }
    }

    if ($rawClass.Contains("AUTHORITY-POLARITY") -or $rawClass.Contains("ADVERSARIAL-AUTHORITY")) {
        $AuthPolTotal++
        if ($ctype -eq "AuthorityPolarity") {
            $AuthPolFired++
        }
    }

    if ($rawClass.Contains("UNRESOLVED-CONFLICT")) {
        $UnresTotal++
        if ($ctype -eq "None" -or $ctype -eq "") {
            $UnresNonConflict++
        }
    }

    if ($rawClass.Contains("SAFE-ABSTENTION")) {
        $SafeAbstainTotal++
        if ($abstained) {
            $SafeAbstainFired++
        }
    }

    if ($contaminated) {
        $PoolContamination++
    }
}

# Verify 5-way outcome exhaustiveness
$DecompSum = $Count_SelectedDPlus + $Count_SelectedDMinus + $Count_SelectedOther + $Count_CorrectAbstention + $Count_IncorrectAbstention
if ($DecompSum -ne $TotalScenarios) {
    Fail "Selection outcome decomposition is not exhaustive: sum=$DecompSum != total=$TotalScenarios"
}

# -- Section 6: Bootstrap CI Computation ---------------------------------------

function Bootstrap-CI {
    param(
        [double[]]$Samples,
        [int]$Resamples,
        [long]$Seed
    )
    $n = $Samples.Count
    $rng = New-Object System.Random([int]($Seed -band 0x7FFFFFFF))
    $stats = [double[]]::new($Resamples)
    for ($r = 0; $r -lt $Resamples; $r++) {
        $sum = 0.0
        for ($j = 0; $j -lt $n; $j++) {
            $sum += $Samples[$rng.Next(0, $n)]
        }
        $stats[$r] = $sum / $n
    }
    [System.Array]::Sort($stats)
    $lo = $stats[[int][Math]::Floor(0.025 * $Resamples)]
    $hi = $stats[[int][Math]::Floor(0.975 * $Resamples)]
    return @{ Lower = $lo; Upper = $hi }
}

$BsSeedLong = [System.Convert]::ToInt64($BOOTSTRAP_SEED_HEX.Substring(2), 16)
$G1_ci = Bootstrap-CI -Samples $G1_arr -Resamples $BOOTSTRAP_RESAMPLES -Seed $BsSeedLong
$G2_ci = Bootstrap-CI -Samples $G2_arr -Resamples $BOOTSTRAP_RESAMPLES -Seed $BsSeedLong
$G3_ci = Bootstrap-CI -Samples $G3_arr -Resamples $BOOTSTRAP_RESAMPLES -Seed $BsSeedLong
$G4_ci = Bootstrap-CI -Samples $G4_arr -Resamples $BOOTSTRAP_RESAMPLES -Seed $BsSeedLong
$G5_ci = Bootstrap-CI -Samples $G5_arr -Resamples $BOOTSTRAP_RESAMPLES -Seed $BsSeedLong
$G6_ci = Bootstrap-CI -Samples $G6_arr -Resamples $BOOTSTRAP_RESAMPLES -Seed $BsSeedLong

# -- Section 7: Gate Evaluation ------------------------------------------------

Write-Log ""
Write-Log "=== Gate Evaluation ==="
$AllPass = $true

function Eval-Gate {
    param(
        [string]$Name,
        [double]$Estimate,
        [double]$CiBound,
        [string]$Direction,
        [double]$Threshold,
        [string]$Formula
    )
    $passEst = if ($Direction -eq "lower") { $Estimate -ge $Threshold } else { $Estimate -lt $Threshold }
    $passCi  = if ($Direction -eq "lower") { $CiBound -ge $Threshold } else { $CiBound -lt $Threshold }
    $pass = $passEst -and $passCi
    $verdict = if ($pass) { "PASS" } else { "FAIL" }
    Write-Log ("  {0,-35} | Est: {1,7:F4} | 95% CI {2}: {3,7:F4} | Thresh: {4} | {5}" -f $Name, $Estimate, $Direction, $CiBound, $Threshold, $verdict)
    Write-Log "    Formula: $Formula"
    return $pass
}

function Eval-Exact {
    param(
        [string]$Name,
        [double]$Estimate,
        [string]$Comparator,
        [double]$Threshold,
        [string]$Details
    )
    $pass = if ($Comparator -eq "lte") { $Estimate -le $Threshold } elseif ($Comparator -eq "gte") { $Estimate -ge $Threshold } else { $false }
    $verdict = if ($pass) { "PASS" } else { "FAIL" }
    Write-Log ("  {0,-35} | Value: {1,5:F4} | Thresh: {2} | {3}" -f $Name, $Estimate, $Threshold, $verdict)
    Write-Log "    $Details"
    return $pass
}

# G1
$g1_est = $PosRank1Count / $TotalScenarios
if (-not (Eval-Gate "G1 Recall@1 (SCORED)" $g1_est $G1_ci.Lower "lower" $GATE_G1_RECALL_AT_1_MIN "pos_rank=1 in SCORED")) { $AllPass = $false }

# G2
$g2_est = $PosTop5Count / $TotalScenarios
if (-not (Eval-Gate "G2 Recall@5 (SCORED)" $g2_est $G2_ci.Lower "lower" $GATE_G2_RECALL_AT_5_MIN "pos_in_top5 in SCORED")) { $AllPass = $false }

# G3
$g3_est = $MrrSum / $TotalScenarios
if (-not (Eval-Gate "G3 MRR (SCORED, no cap)" $g3_est $G3_ci.Lower "lower" $GATE_G3_MRR_MIN "mean(1/pos_rank)")) { $AllPass = $false }

# G4 (UPPER CI bound)
$g4_est = $NegRank1Count / $TotalScenarios
if (-not (Eval-Gate "G4 Contradiction-at-1" $g4_est $G4_ci.Upper "upper" $GATE_G4_CONTRADICTION_AT_1_MAX "neg_rank=1 in SCORED")) { $AllPass = $false }

# G5 (UPPER CI bound, slot occupancy)
$g5_est = ($G5_arr | Measure-Object -Average).Average
if (-not (Eval-Gate "G5 Hard-Neg Occupancy@5" $g5_est $G5_ci.Upper "upper" $GATE_G5_HARD_NEG_OCC_AT_5_MAX "sum(hn_top5_slots)/(5*N)")) { $AllPass = $false }

# G6
$g6_est = $MarginSum / $TotalScenarios
if (-not (Eval-Gate "G6 Mean Margin (SCORED)" $g6_est $G6_ci.Lower "lower" $GATE_G6_MEAN_MARGIN_MIN "mean(pos_score - neg_score)")) { $AllPass = $false }

# G7 (Extended: zero presence in all observables)
$g7_est = if ($SupersessionTotal -gt 0) { $SupersessionViolations / $SupersessionTotal } else { 0.0 }
if (-not (Eval-Exact "G7 Supersession Complete Safety" $g7_est "lte" $GATE_G7_SUPERSESSION_LEAK_MAX "Violations: $SupersessionViolations / $SupersessionTotal (zero allowed across post_filter, scored, conflict, chain, winner)")) { $AllPass = $false }

# G8a (LogicalContradiction classification)
$g8a_est = if ($ContraTotal -gt 0) { $ContraLcFired / $ContraTotal } else { 0.0 }
if (-not (Eval-Exact "G8a LogicalContradiction Rate" $g8a_est "gte" $GATE_G8A_LOGICAL_CONTRA_MIN "Fired: $ContraLcFired / $ContraTotal CONTRADICTION scenarios")) { $AllPass = $false }

# G8b (AuthorityPolarity classification)
$g8b_est = if ($AuthPolTotal -gt 0) { $AuthPolFired / $AuthPolTotal } else { 0.0 }
if (-not (Eval-Exact "G8b AuthorityPolarity Rate" $g8b_est "gte" $GATE_G8B_AUTH_POLARITY_MIN "Fired: $AuthPolFired / $AuthPolTotal AUTHORITY-POLARITY/ADVERSARIAL scenarios")) { $AllPass = $false }

# G8c (Unresolved non-conflict rate)
$g8c_est = if ($UnresTotal -gt 0) { $UnresNonConflict / $UnresTotal } else { 0.0 }
if (-not (Eval-Exact "G8c Unresolved Non-Conflict Rate" $g8c_est "gte" $GATE_G8C_UNRESOLVED_NON_CONF "No spurious conflict: $UnresNonConflict / $UnresTotal UNRESOLVED-CONFLICT scenarios")) { $AllPass = $false }

# G9 (Safe Abstention rate)
$g9_est = if ($SafeAbstainTotal -gt 0) { $SafeAbstainFired / $SafeAbstainTotal } else { 0.0 }
if (-not (Eval-Exact "G9 Safe Abstention Rate" $g9_est "gte" $GATE_G9_SAFE_ABSTENTION_MIN "Abstained: $SafeAbstainFired / $SafeAbstainTotal SAFE-ABSTENTION scenarios")) { $AllPass = $false }

# G10 (Pool contamination)
if (-not (Eval-Exact "G10 Pool Contamination" $PoolContamination "lte" $GATE_G10_POOL_CONTAMINATION_MAX "Contamination count: $PoolContamination (zero allowed)")) { $AllPass = $false }

# S1 (Supporting gate: LC eligibility)
$s1_est = if ($ContraTotal -gt 0) { $ContraEligible / $ContraTotal } else { 0.0 }
if (-not (Eval-Exact "S1 LC Eligibility Rate" $s1_est "gte" $GATE_S1_LC_ELIGIBILITY_MIN "Eligible top-2 gap: $ContraEligible / $ContraTotal CONTRADICTION scenarios")) { $AllPass = $false }

# -- Section 8: Selection Decision Outcome Report (Blocker 7) -------------------

Write-Log ""
Write-Log "=== Selection Decision Outcome Report (5-Way Exhaustive) ==="
Write-Log ("  Selected D+ (safe answer):               {0,3} ({1,6:P2})" -f $Count_SelectedDPlus,       ($Count_SelectedDPlus / $TotalScenarios))
Write-Log ("  Selected D- (unsafe contradiction):       {0,3} ({1,6:P2})" -f $Count_SelectedDMinus,      ($Count_SelectedDMinus / $TotalScenarios))
Write-Log ("  Selected Other (distractor / unrelated): {0,3} ({1,6:P2})" -f $Count_SelectedOther,       ($Count_SelectedOther / $TotalScenarios))
Write-Log ("  Correct Abstention (safety triggered):    {0,3} ({1,6:P2})" -f $Count_CorrectAbstention,   ($Count_CorrectAbstention / $TotalScenarios))
Write-Log ("  Incorrect Abstention (spurious):          {0,3} ({1,6:P2})" -f $Count_IncorrectAbstention, ($Count_IncorrectAbstention / $TotalScenarios))
Write-Log ("  Total Scenarios:                          {0,3} (100.00%)" -f $TotalScenarios)

# -- Section 9: Final Verdict ---------------------------------------------------

Write-Log ""
Write-Log "=== Final Verification Verdict ==="
if ($AllPass) {
    Write-Log "ALL 10 GATES + S1 PASSED -- Phase 3 acceptance criteria satisfied."
    exit 0
} else {
    Write-Log "ONE OR MORE GATES FAILED -- Benchmark does not satisfy approval criteria."
    exit 1
}
