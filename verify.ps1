<#
.SYNOPSIS
    Universal Quality Gate for opc-cli.
.DESCRIPTION
    Runs cargo fmt, clippy, doc tests, and standard workspace tests.
    Halts execution strictly on any non-zero exit code.
#>

$ErrorActionPreference = 'Stop'

# Ensure we exit with the correct code if any native command fails
$ErrorView = 'NormalView'

function Require-ZeroExit {
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Verification Failed ❌" -ForegroundColor Red
        exit $LASTEXITCODE
    }
}

Write-Host "Running Verification Pipeline..." -ForegroundColor Cyan

Write-Host "`n>>> Formatter Check" -ForegroundColor Yellow
cargo fmt --all -- --check
Require-ZeroExit

Write-Host "`n>>> Linter Check" -ForegroundColor Yellow
cargo clippy --workspace --all-targets --all-features -- -D warnings
Require-ZeroExit

Write-Host "`n>>> Doc Compilation Check (cargo test --doc)" -ForegroundColor Yellow
cargo test --doc --workspace
Require-ZeroExit

Write-Host "`n>>> Unit & Integration Tests" -ForegroundColor Yellow
cargo test --workspace
Require-ZeroExit

Write-Host "`nAll Gates Passed! ✅" -ForegroundColor Green
exit 0
