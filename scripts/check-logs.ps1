<#
.SYNOPSIS
    Inspect application logs for errors and warnings.
.DESCRIPTION
    Dynamically discovers the latest log file(s) in the logs/ directory,
    filters by severity level, and outputs a structured summary with
    colorized entries for human and AI diagnostics.
.PARAMETER Lines
    Number of matching tail lines to display. Default: 50.
.PARAMETER Level
    Minimum severity filter: TRACE, DEBUG, INFO, WARN, or ERROR. Default: WARN.
.PARAMETER All
    Scan all log files, not just the latest.
#>

param(
    [int]$Lines = 50,
    [ValidateSet("TRACE", "DEBUG", "INFO", "WARN", "ERROR")]
    [string]$Level = "WARN",
    [switch]$All
)

$ErrorActionPreference = 'Stop'

# --- Discovery ---
$logDir = Join-Path (Split-Path -Parent $PSScriptRoot) "logs"

if (-not (Test-Path $logDir)) {
    Write-Host "No logs/ directory found." -ForegroundColor Red
    exit 1
}

$logFiles = Get-ChildItem $logDir -File | Sort-Object LastWriteTime -Descending

if ($logFiles.Count -eq 0) {
    Write-Host "No log files found in $logDir" -ForegroundColor Red
    exit 1
}

if (-not $All) {
    $logFiles = @($logFiles[0])
}

# --- Severity hierarchy ---
$severityOrder = @{ "TRACE" = 0; "DEBUG" = 1; "INFO" = 2; "WARN" = 3; "ERROR" = 4 }
$minSeverity = $severityOrder[$Level]
# Tracing format: "2026-02-22T03:13:24.527Z  INFO module::path: message"
# Anchor severity to the tracing timestamp prefix to avoid false positives
# from data containing words like "Error" in tag names.
$levelPattern = switch ($Level) {
    "TRACE" { '\d{4}-\d{2}-\d{2}T[\d:.]+Z\s+(TRACE|DEBUG|INFO|WARN|ERROR)' }
    "DEBUG" { '\d{4}-\d{2}-\d{2}T[\d:.]+Z\s+(DEBUG|INFO|WARN|ERROR)' }
    "INFO"  { '\d{4}-\d{2}-\d{2}T[\d:.]+Z\s+(INFO|WARN|ERROR)' }
    "WARN"  { '\d{4}-\d{2}-\d{2}T[\d:.]+Z\s+(WARN|ERROR)' }
    "ERROR" { '\d{4}-\d{2}-\d{2}T[\d:.]+Z\s+ERROR' }
}

# --- Process each file ---
$totalLines = 0
$totalTrace = 0
$totalDebug = 0
$totalInfo = 0
$totalWarn = 0
$totalError = 0
$hasErrors = $false
$matchedLines = @()

foreach ($file in $logFiles) {
    $content = Get-Content $file.FullName
    $fileLines = $content.Count
    $totalLines += $fileLines

    # Count severities using anchored patterns
    $traceCount = ($content | Select-String -Pattern '\d{4}-\d{2}-\d{2}T[\d:.]+Z\s+TRACE').Count
    $debugCount = ($content | Select-String -Pattern '\d{4}-\d{2}-\d{2}T[\d:.]+Z\s+DEBUG').Count
    $infoCount  = ($content | Select-String -Pattern '\d{4}-\d{2}-\d{2}T[\d:.]+Z\s+INFO').Count
    $warnCount  = ($content | Select-String -Pattern '\d{4}-\d{2}-\d{2}T[\d:.]+Z\s+WARN').Count
    $errorCount = ($content | Select-String -Pattern '\d{4}-\d{2}-\d{2}T[\d:.]+Z\s+ERROR').Count

    $totalTrace += $traceCount
    $totalDebug += $debugCount
    $totalInfo  += $infoCount
    $totalWarn  += $warnCount
    $totalError += $errorCount

    if ($errorCount -gt 0) { $hasErrors = $true }

    # Collect matching lines
    $matches = $content | Select-String -Pattern $levelPattern
    $matchedLines += $matches | Select-Object -Last $Lines

    # --- File Header ---
    Write-Host "`n========================================" -ForegroundColor Cyan
    Write-Host " Log : $($file.Name)" -ForegroundColor Cyan
    Write-Host " Size: $([math]::Round($file.Length / 1KB, 1)) KB" -ForegroundColor Cyan
    Write-Host " Lines: $fileLines" -ForegroundColor Cyan
    Write-Host " TRACE: $traceCount | DEBUG: $debugCount | INFO: $infoCount | WARN: $warnCount | ERROR: $errorCount" -ForegroundColor Cyan
    Write-Host "========================================" -ForegroundColor Cyan
}

# --- Output matched lines ---
Write-Host "`n--- Last $Lines $Level+ entries ---`n" -ForegroundColor Yellow

$tail = $matchedLines | Select-Object -Last $Lines
foreach ($line in $tail) {
    $text = $line.Line
    if ($text -match '\d{4}-\d{2}-\d{2}T[\d:.]+Z\s+ERROR') {
        Write-Host $text -ForegroundColor Red
    } elseif ($text -match '\d{4}-\d{2}-\d{2}T[\d:.]+Z\s+WARN') {
        Write-Host $text -ForegroundColor DarkYellow
    } elseif ($text -match '\d{4}-\d{2}-\d{2}T[\d:.]+Z\s+INFO') {
        Write-Host $text -ForegroundColor Gray
    } elseif ($text -match '\d{4}-\d{2}-\d{2}T[\d:.]+Z\s+DEBUG') {
        Write-Host $text -ForegroundColor DarkCyan
    } else {
        Write-Host $text -ForegroundColor DarkGray
    }
}

# --- Summary ---
Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host " Summary" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host " Files scanned : $($logFiles.Count)" -ForegroundColor Cyan
Write-Host " Total lines   : $totalLines" -ForegroundColor Cyan
Write-Host " TRACE          : $totalTrace" -ForegroundColor DarkGray
Write-Host " DEBUG          : $totalDebug" -ForegroundColor DarkCyan
Write-Host " INFO           : $totalInfo" -ForegroundColor Gray
Write-Host " WARN           : $totalWarn" -ForegroundColor DarkYellow
Write-Host " ERROR          : $totalError" -ForegroundColor Red
Write-Host "========================================" -ForegroundColor Cyan

if ($hasErrors) {
    Write-Host "`nErrors detected." -ForegroundColor Red
    exit 1
} else {
    Write-Host "`nNo errors found." -ForegroundColor Green
    exit 0
}
