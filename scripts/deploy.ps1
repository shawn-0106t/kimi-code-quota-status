﻿# quota-status deployment script (P1 automation): build -> deploy -> sha256 verification.
# Usage: powershell -File scripts/deploy.ps1        (works on Windows PowerShell 5.1)
#       pwsh -File scripts/deploy.ps1 [-DstDir C:\tools]   (pwsh 7 also works)
# Default deploy location ~/.kimi-code/bin (maintainer layout); the README "Installation" section example layout (e.g.
# C:\tools) can be passed via -DstDir to override. Mirrors the docs/HANDOFF.md §5 manual procedure:
# "after a rebuild, copy to that path again; if the file is in use by a running session, rename the old exe to quota-status.exe.old first, then copy the new one".

param(
    # Deploy directory; defaults to the maintainer layout, README example layout via parameter override
    [string]$DstDir = (Join-Path $HOME ".kimi-code\bin")
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$src = Join-Path $repoRoot "target\release\quota-status.exe"
$dst = Join-Path $DstDir "quota-status.exe"
$old = Join-Path $DstDir "quota-status.exe.old"

# 1. Build (--locked, same as CI). PS 5.1 pitfall (confirmed by review m-2 testing): with EAP=Stop,
#    native command stderr lines (cargo progress output goes to stderr) throw NativeCommandError
#    as soon as redirected via 2>&1, and ForEach-Object stringification cannot stop it - the right
#    fix is to lower EAP to Continue locally so stderr lines flow through the pipeline; success is judged solely by $LASTEXITCODE
Push-Location $repoRoot
try {
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        cargo build --release --locked 2>&1 | ForEach-Object { "$_" }
    } finally {
        $ErrorActionPreference = $prevEap
    }
    if ($LASTEXITCODE -ne 0) { throw "cargo build --release failed, aborting deployment" }
} finally {
    Pop-Location
}

# 2. Ensure the deploy directory exists
New-Item -ItemType Directory -Force -Path $DstDir | Out-Null

# 3. Copy; guard rails on failure (review M-1):
#    - No old exe at the deploy location (first deployment) -> fail outright, nothing to fall back on;
#    - Source artifact missing (broken build output) -> do not touch the deploy location;
#    - Old exe in use (the real fallback scenario) -> rename to .old and retry the copy;
#    - Fallback copy still fails -> roll back .old -> dst; the deploy location must never end up empty (the statusline must not break)
try {
    Copy-Item $src $dst -Force
} catch {
    if (-not (Test-Path $dst)) {
        throw "Copy failed and no old exe at the deploy location (first-deployment scenario), no fallback possible: $_"
    }
    if (-not (Test-Path $src)) {
        throw "Copy failed and source artifact missing (broken build output), deploy location untouched: $_"
    }
    Write-Host "Direct copy failed (old exe may be in use by a running session), using the rename fallback..."
    $bak = $old
    if (Test-Path $old) {
        try {
            Remove-Item $old -Force
        } catch {
            # Historical .old itself is locked (rare): use a timestamped backup name instead, do not abort deployment
            $bak = "$old.$(Get-Date -Format yyyyMMdd-HHmmss)"
        }
    }
    Move-Item $dst $bak -Force
    try {
        Copy-Item $src $dst -Force
    } catch {
        Move-Item $bak $dst -Force
        throw "Fallback copy still failed; rolled back the old exe to keep the deploy location usable; root cause: $_"
    }
}

# 4. sha256 verification: success only when source and deploy location match
$srcHash = (Get-FileHash $src -Algorithm SHA256).Hash
$dstHash = (Get-FileHash $dst -Algorithm SHA256).Hash
if ($srcHash -ne $dstHash) { throw "sha256 mismatch, deployment not in effect: src=$srcHash dst=$dstHash" }

Write-Host "Deployment complete: $dst"
Write-Host "sha256 : $dstHash"
