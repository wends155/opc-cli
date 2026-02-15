$env:PATH += ";C:\bin\portable-msvc\msvc\VC\Tools\MSVC\14.50.35717\bin\Hostx64\x64"
cargo fmt -- --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

cargo clippy -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

cargo test
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Verification Passed! ✅" -ForegroundColor Green
