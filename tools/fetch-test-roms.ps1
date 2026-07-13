<#
.SYNOPSIS
  Clone the open-source WonderSwan hardware test-ROM projects into the gitignored
  fixtures/test-roms/ directory.

.DESCRIPTION
  Downloading is an explicit, operator-run step. By default this script only
  prints what it would clone (a dry run). Pass -Run to actually clone. Review
  each project's license first (see ../docs/TEST_ROMS.md); the built .ws binaries
  are treated as non-redistributable and stay out of the repo.

.EXAMPLE
  powershell -File tools/fetch-test-roms.ps1            # dry run, lists sources
  powershell -File tools/fetch-test-roms.ps1 -Run       # clone sources
#>
[CmdletBinding()]
param(
    [switch]$Run
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $repoRoot 'fixtures\test-roms'

$sources = @(
    @{ Name = 'ws-test-suite'; Url = 'https://github.com/asiekierka/ws-test-suite.git' },
    @{ Name = 'WSCpuTest';     Url = 'https://github.com/FluBBaOfWard/WSCpuTest.git' },
    @{ Name = 'ARMV30MZ';      Url = 'https://github.com/FluBBaOfWard/ARMV30MZ.git' }
)

Write-Host "Destination: $dest"
if (-not $Run) {
    Write-Host "DRY RUN. Would clone (re-run with -Run to proceed):`n"
    $sources | ForEach-Object { Write-Host "  $($_.Name)  <-  $($_.Url)" }
    Write-Host "`nBuilding the ROMs needs the WonderSwan toolchain (wonderful-toolchain / wf-gcc)."
    Write-Host "Some projects publish prebuilt .ws files in their GitHub releases."
    return
}

if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    throw "git not found on PATH."
}
New-Item -ItemType Directory -Force -Path $dest | Out-Null

foreach ($s in $sources) {
    $target = Join-Path $dest $s.Name
    if (Test-Path $target) {
        Write-Host "skip (exists): $($s.Name)"
        continue
    }
    Write-Host "clone: $($s.Name)"
    git clone --depth 1 $s.Url $target
}

Write-Host "`nDone. Built/prebuilt .ws files stay gitignored; record hashes with *.sha256 manifests."
