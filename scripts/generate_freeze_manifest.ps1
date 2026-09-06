# scripts/generate_freeze_manifest.ps1
# Generates cryptographic freeze manifest for Phase 3 approval

param(
    [string]$OutFile = "phase3_freeze_manifest.txt",
    [string]$FreezeCommit = "bc6634047cb900f90fd9832a6cf83793e885d1dc"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)

$files = @(
    "cmhm_core/src/retrieval.rs",
    "cmhm_core/src/engine.rs",
    "cmhm_core/src/context/runtime.rs",
    "cmhm_core/src/text_embed.rs",
    "cmhm_mcp/src/schema.rs",
    "cmhm_mcp/src/tools.rs",
    "cmhm_mcp/src/lib.rs",
    "cmhm_mcp/tests/tool_tests.rs",
    "cmhm_core/tests/phase3_synthetic_eval_tests.rs",
    "scripts/audit_corpus_overlap.ps1",
    "scripts/verify_phase3_approval.ps1"
)

$commit = $FreezeCommit
$lines = [System.Collections.Generic.List[string]]::new()
$lines.Add("=== Phase 3 Implementation Freeze Manifest ===")
$lines.Add("Freeze Commit: $commit")
$lines.Add("Timestamp: $(Get-Date -Format 'o')")
$lines.Add("")
$lines.Add("| File Relative Path | SHA-256 Checksum |")
$lines.Add("|---|---|")

foreach ($rel in $files) {
    $abs = Join-Path $RepoRoot $rel
    if (-not (Test-Path $abs)) {
        throw "Manifest file missing: $abs"
    }
    $h = (Get-FileHash $abs -Algorithm SHA256).Hash.ToUpper()
    $lines.Add('| `' + $rel + '` | `' + $h + '` |')
    Write-Host "$rel : $h"
}

$outAbs = Join-Path $RepoRoot $OutFile
Set-Content -Path $outAbs -Value ($lines -join "`r`n")
Write-Host ""
Write-Host "Manifest written to: $outAbs"
