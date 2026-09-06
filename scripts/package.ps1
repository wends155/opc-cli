<#
.SYNOPSIS
    Universal Task & Packaging Dispatcher for opc-cli.
.DESCRIPTION
    Provides a single PowerShell entry point for all workspace build,
    verification, packaging, log inspection, and release operations.
.PARAMETER Task
    The operation to execute: debug, release, build, test, verify, package, package-win7, logs, commit, release-merge.
.PARAMETER Message
    Optional message parameter passed to commit or release-merge tasks.
#>

[CmdletBinding()]
param (
    [ValidateSet("debug", "release", "build", "test", "verify", "package", "package-win7", "logs", "commit", "release-merge")]
    [string]$Task = "debug",
    [string]$Message
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $RepoRoot

function New-ReleasePackage {
    param (
        [Parameter(Mandatory=$true)]
        [string]$DistDir,
        [Parameter(Mandatory=$true)]
        [string]$ZipPath,
        [string]$ExePath = "target/release/opc-cli.exe",
        [string[]]$AdditionalFiles = @()
    )

    if (Test-Path $DistDir) { Remove-Item -Recurse -Force $DistDir }
    New-Item -ItemType Directory -Force -Path $DistDir | Out-Null
    Copy-Item $ExePath "$DistDir/"
    Copy-Item -ErrorAction SilentlyContinue "target/release/opc-cli.pdb" "$DistDir/"
    Copy-Item "README.md" "$DistDir/"
    Copy-Item "LICENSE" "$DistDir/"
    Copy-Item "THIRD_PARTY_LICENSES.md" "$DistDir/"

    foreach ($file in $AdditionalFiles) {
        if (Test-Path $file) {
            Copy-Item $file "$DistDir/"
        }
    }

    if (Test-Path $ZipPath) { Remove-Item $ZipPath }
    Compress-Archive -Path "$DistDir/*" -DestinationPath $ZipPath -Force
    Write-Host "Package created: $ZipPath" -ForegroundColor Green
}

switch ($Task) {
    "debug" {
        cargo build
    }
    "release" {
        cargo build --release
    }
    "build" {
        cargo build --release
    }
    "test" {
        cargo test --workspace
    }
    "verify" {
        & "$PSScriptRoot/verify.ps1"
    }
    "package" {
        cargo build --release --bin opc-cli
        New-ReleasePackage -DistDir "dist/opc-cli-x64" -ZipPath "dist/opc-cli-x64.zip"
    }
    "package-win7" {
        & "$PSScriptRoot/package-win7.ps1"
    }
    "logs" {
        & "$PSScriptRoot/check-logs.ps1"
    }
    "commit" {
        if ($Message) {
            & "$PSScriptRoot/commit.ps1" -Message $Message
        } else {
            & "$PSScriptRoot/commit.ps1"
        }
    }
    "release-merge" {
        if ($Message) {
            & "$PSScriptRoot/Merge-ToMain.ps1" -Message $Message
        } else {
            & "$PSScriptRoot/Merge-ToMain.ps1"
        }
    }
}
