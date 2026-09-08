# scripts/audit_corpus_overlap.ps1
# Phase 3 Independent Overlap Audit -- v1.4
#
# Audits a candidate Phase 3 approval corpus against THREE prohibited corpora:
#   1. cmhm_core/tests/dprime_bench.rs (15-query rejection set)
#   2. cmhm_core/tests/phase3_held_out_bench.rs (compromised 32-query corpus)
#   3. ATTEMPT-1 QUERY LIST (40 queries, hardcoded from scaffold commit e989a91)
#      Enforcement: verbatim exact match after case+whitespace normalization.
#      Reviewer ruling (2026-09-08): "zero attempt-1 query reuse" is non-negotiable;
#      fresh means fresh — not minor paraphrases of attempt-1 ancestors either.
#      The Jaccard + verbatim checks (sources 1 & 2) also apply to source 3.
#
# Protocol reference: docs/phase3_approval_protocol_v1.8.md
#
# Measurable Novelty Rules:
#   - Normalization: lowercase, alphanumeric + spaces only, collapsed whitespace.
#   - Tokenization: word-level sliding 3-grams (word trigrams).
#   - Jaccard threshold: Jaccard index >= 0.30 is an OVERLAP VIOLATION.
#   - Verbatim sequence: any shared verbatim sequence of >= 6 consecutive words is an OVERLAP VIOLATION.
#   - Exact query collision: candidate query identical to any reference query is an OVERLAP VIOLATION.
#   - Attempt-1 exact match: candidate query that normalizes identically to any attempt-1 query
#     is an OVERLAP VIOLATION (checked separately for clear failure attribution).
#   - Internal duplicates: candidate corpus must have zero duplicate queries.
#   - Fail-closed: extraction errors, missing files, or empty corpora exit with non-zero code.
#
# Usage:
#   .\scripts\audit_corpus_overlap.ps1 -SyntheticSelfTest
#   .\scripts\audit_corpus_overlap.ps1 -CandidateCorpusPath <path>

param(
    [string]$CandidateCorpusPath = "",
    [string]$ReferenceRejectionPath = "cmhm_core/tests/dprime_bench.rs",
    [string]$ReferenceCompromisedPath = "cmhm_core/tests/phase3_held_out_bench.rs",
    [double]$MaxAllowedJaccard = 0.30,
    [int]$MinVerbatimWords = 6,
    [switch]$SyntheticSelfTest = $false,
    [string]$AuditLogPath = "phase3_overlap_audit.log"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)

# -- Attempt-1 Query Exclusion List (hardcoded, recoverable from scaffold commit e989a91) ------
# These 40 queries were used in the attempt-1 corpus (ruled INVALID ON ITS FACE).
# Reviewer ruling 2026-09-08: zero verbatim or near-verbatim reuse permitted.
# Hardcoded here so the exclusion is pinned and tamper-evident independent of any file on disk.
$Attempt1Queries = @(
    "Should the systemd unit for the API service enable automatic restarts?",
    "Is connection pooling enabled for the PostgreSQL client?",
    "Is TLS certificate verification enabled for outgoing HTTPS requests?",
    "Is the Kubernetes deployment configured to use a rolling update strategy?",
    "Is MFA enforced for all IAM users?",
    "Should the encryption key be rotated every 90 days?",
    "Is the Raft consensus leader election timeout set to 150ms?",
    "Is debug-level logging enabled for the ingest service?",
    "Is the data volume mounted with noatime?",
    "Is the swap partition encrypted?",
    "What is the current read replica count for the production DB?",
    "Which TLS version is allowed for the public API?",
    "What is the CPU limit for the worker container?",
    "What is the maximum session duration for temporary credentials?",
    "What is the current password policy for service accounts?",
    "What is the quorum size for the etcd cluster?",
    "Should the metrics endpoint be exposed publicly?",
    "What filesystem type is used for the data volume?",
    "What is the kernel sysctl value for net.core.somaxconn?",
    "What is the connection timeout for the Redis client?",
    "What is the keepalive interval for the gRPC connection?",
    "What is the log shipping batch size?",
    "Should the S3 bucket be public?",
    "Is the admin password stored in plaintext?",
    "Is privileged mode enabled for the container?",
    "Is the Raft snapshot compression enabled?",
    "Is the trace sampling rate set to 10%?",
    "Should the IAM role have full administrative access?",
    "What is the default block size for the log volume?",
    "What is the file descriptor limit for the application process?",
    "What is the isolation level for the transactions service?",
    "What is the HTTP keep-alive timeout for the proxy?",
    "Is the IAM role allowed to assume the production role?",
    "What is the mount option for the database volume regarding file locking?",
    "What encryption algorithm is used for data at rest?",
    "What is the image pull policy for the deployment?",
    "What is the heartbeat interval for the consensus group?",
    "What is the log retention period for the audit logs?",
    "What is the backup snapshot frequency for the persistent volume?",
    "What is the systemd service type for the worker process?"
)

# -- Text Normalization and Tokenization ----------------------------------------

function Normalize-Text {
    param([string]$Text)
    if ([string]::IsNullOrWhiteSpace($Text)) { return "" }
    $lower = $Text.ToLowerInvariant()
    $clean = $lower -replace '[^a-z0-9]', ' '
    $collapsed = ($clean -replace '\s+', ' ').Trim()
    return $collapsed
}

function Get-WordTrigrams {
    param([string]$NormalizedText)
    if ([string]::IsNullOrWhiteSpace($NormalizedText)) { return [System.Collections.Generic.HashSet[string]]::new() }
    $words = $NormalizedText -split ' ' | Where-Object { $_ -ne "" }
    $trigrams = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    
    if ($words.Count -lt 3) {
        if ($words.Count -gt 0) {
            [void]$trigrams.Add(($words -join ' '))
        }
        return $trigrams
    }
    
    for ($i = 0; $i -le ($words.Count - 3); $i++) {
        $tri = "$($words[$i]) $($words[$i+1]) $($words[$i+2])"
        [void]$trigrams.Add($tri)
    }
    return $trigrams
}

function Compute-Jaccard {
    param(
        [System.Collections.Generic.HashSet[string]]$SetA,
        [System.Collections.Generic.HashSet[string]]$SetB
    )
    if ($SetA.Count -eq 0 -and $SetB.Count -eq 0) { return 0.0 }
    
    $intersectionCount = 0
    foreach ($item in $SetA) {
        if ($SetB.Contains($item)) {
            $intersectionCount++
        }
    }
    $unionCount = $SetA.Count + $SetB.Count - $intersectionCount
    if ($unionCount -eq 0) { return 0.0 }
    return [double]$intersectionCount / [double]$unionCount
}

function Check-VerbatimOverlap {
    param(
        [string]$NormalizedA,
        [string]$NormalizedB,
        [int]$MinWords = 6
    )
    $wordsA = $NormalizedA -split ' ' | Where-Object { $_ -ne "" }
    if ($wordsA.Count -lt $MinWords) { return $false }
    
    for ($i = 0; $i -le ($wordsA.Count - $MinWords); $i++) {
        $sub = ($wordsA[$i..($i + $MinWords - 1)] -join ' ')
        if ($NormalizedB.Contains($sub)) {
            return $true
        }
    }
    return $false
}

# -- Extraction Helpers ---------------------------------------------------------

function Extract-StringsFromRustFile {
    param(
        [string]$FilePath,
        [string[]]$FieldNames
    )
    if (-not (Test-Path $FilePath)) {
        throw "Extraction failure: file not found: $FilePath"
    }
    $content = Get-Content $FilePath -Raw
    $results = [System.Collections.Generic.List[string]]::new()
    
    foreach ($field in $FieldNames) {
        $pattern = "$field\s*:\s*`"([^`"]+)`""
        $matches = [regex]::Matches($content, $pattern)
        foreach ($m in $matches) {
            $val = $m.Groups[1].Value.Trim()
            if ($val -ne "") {
                $results.Add($val)
            }
        }
    }
    # NOTE: comma operator forces PS to preserve an empty List as a List object
    # rather than unwrapping it to $null (PS pipeline unrolling behaviour).
    return ,$results
}

# -- Synthetic Self-Test Mode --------------------------------------------------

if ($SyntheticSelfTest) {
    Write-Host "=== Running Independent Overlap Auditor Synthetic Self-Tests ==="
    
    # Test 1: Identical strings must yield Jaccard = 1.0 and verbatim match
    $s1 = "how to configure postgresql max connections properly in production cluster"
    $s2 = "how to configure postgresql max connections properly in production cluster"
    $t1 = Get-WordTrigrams (Normalize-Text $s1)
    $t2 = Get-WordTrigrams (Normalize-Text $s2)
    $j1 = Compute-Jaccard $t1 $t2
    if ([Math]::Abs($j1 - 1.0) -gt 1e-6) {
        throw "SelfTest 1 FAILED: identical strings must have Jaccard 1.0, got $j1"
    }
    $v1 = Check-VerbatimOverlap (Normalize-Text $s1) (Normalize-Text $s2) 6
    if (-not $v1) {
        throw "SelfTest 1 FAILED: identical strings must have verbatim overlap"
    }
    Write-Host "  PASS  SelfTest 1: Identical strings detected (Jaccard = 1.0, verbatim = true)"
    
    # Test 2: Paraphrase / Near-duplicate must trigger threshold (>= 0.30)
    $p1 = "configure postgresql connection pool size using pgbouncer in front of master"
    $p2 = "configure postgresql connection pool size using pgbouncer proxy for database"
    $tp1 = Get-WordTrigrams (Normalize-Text $p1)
    $tp2 = Get-WordTrigrams (Normalize-Text $p2)
    $jp = Compute-Jaccard $tp1 $tp2
    if ($jp -lt 0.30) {
        throw "SelfTest 2 FAILED: near-duplicate should exceed 0.30 Jaccard, got $jp"
    }
    Write-Host "  PASS  SelfTest 2: Near-duplicate correctly detected (Jaccard = $("{0:F4}" -f $jp) >= 0.30)"
    
    # Test 3: Completely distinct strings must have Jaccard < 0.10 and no verbatim overlap
    $d1 = "kubernetes pod memory limits and cgroups eviction threshold"
    $d2 = "redis sentinel replication quorum and master failover timeout"
    $td1 = Get-WordTrigrams (Normalize-Text $d1)
    $td2 = Get-WordTrigrams (Normalize-Text $d2)
    $jd = Compute-Jaccard $td1 $td2
    if ($jd -ge 0.10) {
        throw "SelfTest 3 FAILED: distinct strings should have Jaccard < 0.10, got $jd"
    }
    $vd = Check-VerbatimOverlap (Normalize-Text $d1) (Normalize-Text $d2) 6
    if ($vd) {
        throw "SelfTest 3 FAILED: distinct strings should not have verbatim overlap"
    }
    Write-Host "  PASS  SelfTest 3: Distinct strings confirmed disjoint (Jaccard = $("{0:F4}" -f $jd) < 0.10)"
    
    # Test 4: Extraction on missing file must throw (fail closed)
    $threw = $false
    try {
        $null = Extract-StringsFromRustFile "nonexistent_file_xyz.rs" @("query")
    } catch {
        $threw = $true
    }
    if (-not $threw) {
        throw "SelfTest 4 FAILED: missing file did not fail closed"
    }
    Write-Host "  PASS  SelfTest 4: Missing file correctly fails closed"

    # -- Source 3 live-fire tests (prove attempt-1 detection actually fires) ----

    # Build the same norm-set the primary audit builds, using $Attempt1Queries defined above.
    $st_attempt1NormSet = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($q in $Attempt1Queries) {
        [void]$st_attempt1NormSet.Add((Normalize-Text $q))
    }
    if ($st_attempt1NormSet.Count -ne 40) {
        throw "SelfTest 5/6 setup FAILED: expected 40 attempt-1 queries in norm-set, got $($st_attempt1NormSet.Count)"
    }

    # Test 5: Verbatim attempt-1 query MUST be caught by exact normalized match.
    # Using "Should the metrics endpoint be exposed publicly?" (attempt-1 ID 17).
    $st5_query = "Should the metrics endpoint be exposed publicly?"
    $st5_norm  = Normalize-Text $st5_query
    if (-not $st_attempt1NormSet.Contains($st5_norm)) {
        throw "SelfTest 5 FAILED: verbatim attempt-1 query not caught by Source 3 check. Normalized form: '$st5_norm'"
    }
    Write-Host "  PASS  SelfTest 5: Verbatim attempt-1 query correctly caught by Source 3 (norm='$st5_norm')"

    # Test 6: Same query with case and whitespace mangled MUST still be caught after normalization.
    # "Should  THE  Metrics  Endpoint  Be  Exposed  Publicly?" -- mixed case, double spaces.
    $st6_query = "Should  THE  Metrics  Endpoint  Be  Exposed  Publicly?"
    $st6_norm  = Normalize-Text $st6_query
    if ($st6_norm -ne $st5_norm) {
        throw "SelfTest 6 FAILED: mangled query did not normalize to same form as verbatim. Got '$st6_norm', expected '$st5_norm'"
    }
    if (-not $st_attempt1NormSet.Contains($st6_norm)) {
        throw "SelfTest 6 FAILED: case/whitespace-mangled attempt-1 query not caught by Source 3 check. Normalized form: '$st6_norm'"
    }
    Write-Host "  PASS  SelfTest 6: Case/whitespace-mangled attempt-1 query correctly caught by Source 3 (norm='$st6_norm')"

    # Sanity check: a genuinely fresh query must NOT appear in the norm-set.
    $st_fresh = "Which RAID level should the backup array use?"
    $st_fresh_norm = Normalize-Text $st_fresh
    if ($st_attempt1NormSet.Contains($st_fresh_norm)) {
        throw "SelfTest 6b FAILED: fresh query '$st_fresh' incorrectly matched against attempt-1 set"
    }
    Write-Host "  PASS  SelfTest 6b: Fresh query correctly NOT matched (norm='$st_fresh_norm')"

    Write-Host "=== All 6 Overlap Auditor Synthetic Self-Tests PASSED ==="
    exit 0
}

# -- Primary Audit Execution ---------------------------------------------------

if ($CandidateCorpusPath -eq "") {
    Write-Error "CandidateCorpusPath is required when not running with -SyntheticSelfTest."
    exit 1
}

$AbsCandidate = if ([System.IO.Path]::IsPathRooted($CandidateCorpusPath)) { $CandidateCorpusPath } else { Join-Path $RepoRoot $CandidateCorpusPath }
$AbsRefRej    = if ([System.IO.Path]::IsPathRooted($ReferenceRejectionPath)) { $ReferenceRejectionPath } else { Join-Path $RepoRoot $ReferenceRejectionPath }
$AbsRefComp   = if ([System.IO.Path]::IsPathRooted($ReferenceCompromisedPath)) { $ReferenceCompromisedPath } else { Join-Path $RepoRoot $ReferenceCompromisedPath }
$AbsLog       = if ([System.IO.Path]::IsPathRooted($AuditLogPath)) { $AuditLogPath } else { Join-Path $RepoRoot $AuditLogPath }

Write-Host "=== Phase 3 Independent Overlap Audit ==="
Write-Host "Candidate:   $AbsCandidate"
Write-Host "Reference 1: $AbsRefRej"
Write-Host "Reference 2: $AbsRefComp"
Write-Host "Thresholds:  Jaccard < $MaxAllowedJaccard, Verbatim < $MinVerbatimWords words"

# Check files exist
if (-not (Test-Path $AbsCandidate)) {
    Write-Error "FAIL CLOSED: Candidate corpus not found at $AbsCandidate"
    exit 1
}
if (-not (Test-Path $AbsRefRej)) {
    Write-Error "FAIL CLOSED: Reference rejection corpus not found at $AbsRefRej"
    exit 1
}
if (-not (Test-Path $AbsRefComp)) {
    Write-Error "FAIL CLOSED: Reference compromised corpus not found at $AbsRefComp"
    exit 1
}

# Extract candidate elements
# Note: approval bench uses pos_text/neg_text; held-out bench uses positive_text/hard_neg_text.
# Both aliases included so either corpus format is covered.
$candidateQueries = Extract-StringsFromRustFile $AbsCandidate @("query")
$candidateTexts   = Extract-StringsFromRustFile $AbsCandidate @("query", "positive_text", "hard_neg_text", "pos_text", "neg_text")

if ($candidateQueries.Count -lt 40) {
    Write-Error "FAIL CLOSED: Candidate corpus has $($candidateQueries.Count) queries; minimum required is 40."
    exit 1
}

# Check internal duplicates in candidate corpus
$uniqueQueries = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
foreach ($q in $candidateQueries) {
    $norm = Normalize-Text $q
    if ($uniqueQueries.Contains($norm)) {
        Write-Error "FAIL CLOSED: Candidate corpus contains internal duplicate query: '$q'"
        exit 1
    }
    [void]$uniqueQueries.Add($norm)
}

# -- Attempt-1 Exact-Match Check (Source 3: hardcoded exclusion list) ----------
# Fails closed if any candidate query normalizes to an attempt-1 query.
# Applied before Jaccard comparison so failure attribution is unambiguous.
Write-Host "Running attempt-1 query exclusion check ($($Attempt1Queries.Count) hardcoded queries)..."
$attempt1NormSet = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
foreach ($q in $Attempt1Queries) {
    [void]$attempt1NormSet.Add((Normalize-Text $q))
}
$attempt1Violations = 0
foreach ($cq in $candidateQueries) {
    $normCq = Normalize-Text $cq
    if ($attempt1NormSet.Contains($normCq)) {
        Write-Host "  ATTEMPT-1 VIOLATION: '$cq'"
        $attempt1Violations++
    }
}
if ($attempt1Violations -gt 0) {
    Write-Error "FAIL CLOSED: $attempt1Violations candidate query/queries match attempt-1 corpus verbatim (case/whitespace-normalized). Zero attempt-1 query reuse is required."
    exit 1
}
Write-Host "  Attempt-1 exclusion check PASSED (0 matches)."

# Extract reference elements (sources 1 & 2)
$refRejTexts  = Extract-StringsFromRustFile $AbsRefRej @("query", "positive_text", "hard_neg_text", "positive", "hard_negative", "negative")
$refCompTexts = Extract-StringsFromRustFile $AbsRefComp @("query", "positive_text", "hard_neg_text")

# Build combined reference pool: sources 1 + 2 + 3 (attempt-1 queries included for Jaccard/verbatim)
$allRefTexts = [System.Collections.Generic.List[string]]::new()
$allRefTexts.AddRange($refRejTexts)
$allRefTexts.AddRange($refCompTexts)
foreach ($q in $Attempt1Queries) { $allRefTexts.Add($q) }

Write-Host "Candidate strings to audit: $($candidateTexts.Count) (Queries: $($candidateQueries.Count))"
Write-Host "Reference strings to compare: $($allRefTexts.Count) (dprime=$($refRejTexts.Count) held-out=$($refCompTexts.Count) attempt-1=$($Attempt1Queries.Count))"

# Audit comparisons (Jaccard + verbatim across all three sources)
$maxJaccard = 0.0
$violations = 0
$totalComparisons = 0

$logLines = [System.Collections.Generic.List[string]]::new()
$logLines.Add("=== Phase 3 Overlap Audit Log (v1.4) ===")
$logLines.Add("Timestamp: $(Get-Date -Format 'o')")
$logLines.Add("Candidate SHA-256:   $((Get-FileHash $AbsCandidate -Algorithm SHA256).Hash)")
$logLines.Add("Ref Rejection SHA:   $((Get-FileHash $AbsRefRej -Algorithm SHA256).Hash)")
$logLines.Add("Ref Compromised SHA: $((Get-FileHash $AbsRefComp -Algorithm SHA256).Hash)")
$logLines.Add("Ref Attempt-1:       40 queries hardcoded at audit-script commit (recoverable from e989a91)")
$logLines.Add("Attempt-1 Exact Matches: $attempt1Violations (must be 0)")
$logLines.Add("Candidate Queries:   $($candidateQueries.Count)")
$logLines.Add("Threshold:           Jaccard < $MaxAllowedJaccard, Verbatim < $MinVerbatimWords words")

foreach ($cand in $candidateTexts) {
    $normCand = Normalize-Text $cand
    $triCand  = Get-WordTrigrams $normCand

    foreach ($ref in $allRefTexts) {
        $normRef = Normalize-Text $ref
        $triRef  = Get-WordTrigrams $normRef
        $totalComparisons++

        # Check exact normalized match
        $exactMatch = ($normCand -eq $normRef)

        $jaccard = Compute-Jaccard $triCand $triRef
        if ($jaccard -gt $maxJaccard) {
            $maxJaccard = $jaccard
        }

        $verbatim = Check-VerbatimOverlap $normCand $normRef $MinVerbatimWords

        if ($exactMatch -or $jaccard -ge $MaxAllowedJaccard -or $verbatim) {
            $violations++
            $logLines.Add("VIOLATION: Jaccard=$("{0:F4}" -f $jaccard), Verbatim=$verbatim, ExactMatch=$exactMatch")
            $logLines.Add("  CANDIDATE: $cand")
            $logLines.Add("  REFERENCE: $ref")
        }
    }
}

$logLines.Add("Total pairwise checks: $totalComparisons")
$logLines.Add("Max Jaccard observed:  $("{0:F4}" -f $maxJaccard)")
$logLines.Add("Total violations:      $violations")

if ($violations -eq 0) {
    $logLines.Add("VERDICT: PASS (Zero overlap violations detected)")
    Set-Content -Path $AbsLog -Value ($logLines -join "`n")
    Write-Host "PASS: Zero overlap violations detected. Max Jaccard = $("{0:F4}" -f $maxJaccard) < $MaxAllowedJaccard."
    exit 0
} else {
    $logLines.Add("VERDICT: FAIL ($violations overlap violations detected)")
    Set-Content -Path $AbsLog -Value ($logLines -join "`n")
    Write-Error "FAIL CLOSED: $violations overlap violations detected. Max Jaccard = $("{0:F4}" -f $maxJaccard) >= $MaxAllowedJaccard."
    exit 1
}
