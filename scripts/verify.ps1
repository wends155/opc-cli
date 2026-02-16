$env:PATH += ";C:\bin\portable-msvc\msvc\VC\Tools\MSVC\14.50.35717\bin\Hostx64\x64"
cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

cargo clippy --workspace -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

cargo test --workspace
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Verification Passed! ✅" -ForegroundColor Green
