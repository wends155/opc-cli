<#
.SYNOPSIS
    Automated Git Commit & Push Pipeline.
.DESCRIPTION
    Mandates a strict commit message parameter.
    Executes the workspace `/verify.ps1` quality gate prior to tracking.
    Safely commits all modified structures and pushes to remote with tracking hooks.
#>

param(
    [Parameter(Mandatory=$true, HelpMessage="Enter a strict conventional commit message.")]
    [string]$Message
)

$ErrorActionPreference = 'Stop'

Write-Host ">>> Checking Quality Gates via verify.ps1..." -ForegroundColor Cyan
pwsh -File .\verify.ps1

if ($LASTEXITCODE -ne 0) {
    Write-Host "`ncommit.ps1 Failed: verify.ps1 encountered a non-zero exit code. Fix the errors before committing." -ForegroundColor Red
    exit $LASTEXITCODE
}

Write-Host "`n>>> Staging modified workspace files (git add .)" -ForegroundColor Yellow
git add .

Write-Host "`n>>> Committing: $Message" -ForegroundColor Yellow
git commit -m $Message

Write-Host "`n>>> Analyzing Upstream & Pushing to Remote..." -ForegroundColor Yellow
$branch = git branch --show-current
git push --set-upstream origin $branch

if ($LASTEXITCODE -eq 0) {
    Write-Host "`nDeployment Pipeline Successful! ✅" -ForegroundColor Green
} else {
    Write-Host "`nDeployment Pipeline Encountered an Error! ❌" -ForegroundColor Red
    exit $LASTEXITCODE
}
