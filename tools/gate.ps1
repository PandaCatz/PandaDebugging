<#
.SYNOPSIS
  Run the full verification gate. Any failure is a red gate; fix before moving on.
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)

Write-Host '== cargo fmt --check =='
cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { throw 'fmt failed' }

Write-Host '== cargo clippy -D warnings =='
cargo clippy --workspace --all-targets --all-features -- -D warnings
if ($LASTEXITCODE -ne 0) { throw 'clippy failed' }

Write-Host '== cargo test (debug) =='
cargo test --workspace --all-targets --all-features
if ($LASTEXITCODE -ne 0) { throw 'debug tests failed' }

Write-Host '== cargo test (release) =='
cargo test --release --workspace
if ($LASTEXITCODE -ne 0) { throw 'release tests failed' }

Write-Host '== ws-cli baseline =='
cargo run --release -q -p ws-cli
if ($LASTEXITCODE -ne 0) { throw 'ws-cli failed' }

Write-Host "`nAll gates green."
