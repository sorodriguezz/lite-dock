#Requires -Version 5.1
<#
.SYNOPSIS
    Start LiteDock in development mode (hot reload).

.DESCRIPTION
    Installs frontend dependencies only if node_modules is missing, then runs
    'npm run tauri dev'. Run from anywhere; the repository root is resolved
    relative to this script. No administrator rights required.

.EXAMPLE
    .\scripts\dev.ps1
#>

$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot  = Split-Path -Parent $ScriptDir
$NodeModules = Join-Path $RepoRoot 'node_modules'

function Write-Step { param([string]$Message) Write-Host "==> $Message" -ForegroundColor Cyan }
function Fail       { param([string]$Message) Write-Host "ERROR: $Message" -ForegroundColor Red; exit 1 }

# Minimal prerequisite check so failures are friendly.
foreach ($tool in 'node','npm') {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        Fail "'$tool' not found on PATH. Install Node.js LTS from https://nodejs.org/."
    }
}

Push-Location $RepoRoot
try {
    if (-not (Test-Path $NodeModules)) {
        Write-Step "node_modules missing — running npm install"
        npm install
        if ($LASTEXITCODE -ne 0) { Fail "'npm install' failed (exit code $LASTEXITCODE)." }
    } else {
        Write-Step "node_modules present — skipping npm install"
    }

    Write-Step "Starting dev server (npm run tauri dev)"
    Write-Host "    Press Ctrl+C to stop." -ForegroundColor DarkGray
    npm run tauri dev
    if ($LASTEXITCODE -ne 0) { Fail "'npm run tauri dev' exited with code $LASTEXITCODE." }
}
finally {
    Pop-Location
}
