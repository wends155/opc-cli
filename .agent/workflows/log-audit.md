---
description: How to audit the application logs for errors and warnings
---

## Steps

// turbo
1. List available log files sorted by most recent:
   ```powershell
   Get-ChildItem logs -File | Sort-Object LastWriteTime -Descending
   ```

// turbo
2. Read the tail of the most recent log file:
   ```powershell
   $latest = Get-ChildItem logs -File | Sort-Object LastWriteTime -Descending | Select-Object -First 1
   Get-Content $latest.FullName -Tail 100
   ```

3. Search for errors and warnings:
   ```powershell
   Select-String -Path $latest.FullName -Pattern "WARN|ERROR" | Select-Object -Last 30
   ```

4. Create an audit report artifact with findings.

> **IMPORTANT**: NEVER guess the log filename from the current date. Always use dynamic discovery (Step 1).
