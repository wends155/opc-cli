#!/bin/sh
# --- Quality Gate ---
set -e

# Delegate entirely to the optimized Windows-native PowerShell gate
pwsh -File scripts/verify.ps1
