param (
    [string]$Task = "debug"
)

switch ($Task) {
    "debug"   { cargo build }
    "release" { cargo build --release }
    "build"   { cargo build --release }
    "test"    { cargo test }
    "package" {
        cargo build --release
        if (!(Test-Path dist)) { New-Item -ItemType Directory -Force -Path dist }
        Copy-Item target/release/opc-cli.exe dist/
        Copy-Item -ErrorAction SilentlyContinue target/release/opc-cli.pdb dist/
        if (Test-Path opc-cli-dist.zip) { Remove-Item opc-cli-dist.zip }
        Compress-Archive -Path dist/* -DestinationPath opc-cli-dist.zip -Force
    }
    Default { Write-Error "Unknown task: $Task" }
}
