#Requires -Version 5.1
<#
.SYNOPSIS
    One-shot Windows build for LiteDock: checks prerequisites, verifies the
    bundled engine rootfs tar, installs frontend deps, and produces the NSIS .exe.

.DESCRIPTION
    Run from anywhere; the script locates the repository root relative to itself.
    Does NOT require administrator rights. On any error it stops with a friendly
    message. The produced installer path is printed at the end.

.EXAMPLE
    .\scripts\build-all.ps1
#>

$ErrorActionPreference = 'Stop'

# --- Resolve repository root (this script lives in <root>\scripts) -------------
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot  = Split-Path -Parent $ScriptDir
$TarPath   = Join-Path $RepoRoot 'src-tauri\resources\litedock-engine.tar'
$BundleDir = Join-Path $RepoRoot 'src-tauri\target\release\bundle\nsis'

function Write-Step { param([string]$Message) Write-Host "==> $Message" -ForegroundColor Cyan }
function Write-Ok   { param([string]$Message) Write-Host "    $Message" -ForegroundColor Green }
function Fail       { param([string]$Message) Write-Host "ERROR: $Message" -ForegroundColor Red; exit 1 }

function Test-Tool {
    param(
        [Parameter(Mandatory)][string]$Command,
        [Parameter(Mandatory)][string]$FriendlyName,
        [Parameter(Mandatory)][string]$HowToInstall
    )
    $exists = Get-Command $Command -ErrorAction SilentlyContinue
    if (-not $exists) {
        Fail "$FriendlyName not found ('$Command' is not on PATH).`n       Install it: $HowToInstall"
    }
    Write-Ok "$FriendlyName found ($($exists.Source))"
}

Write-Host ""
Write-Host "LiteDock build (Windows)" -ForegroundColor White
Write-Host "Repository: $RepoRoot"
Write-Host ""

# --- 1. Prerequisites ----------------------------------------------------------
Write-Step "Checking prerequisites"
Test-Tool -Command 'cargo' -FriendlyName 'Rust toolchain (cargo)' `
    -HowToInstall 'install Rust (stable, MSVC) via https://rustup.rs/'
Test-Tool -Command 'rustc' -FriendlyName 'Rust compiler (rustc)' `
    -HowToInstall 'install Rust (stable, MSVC) via https://rustup.rs/'
Test-Tool -Command 'node' -FriendlyName 'Node.js' `
    -HowToInstall 'install Node.js LTS from https://nodejs.org/'
Test-Tool -Command 'npm' -FriendlyName 'npm' `
    -HowToInstall 'install Node.js LTS (npm is included)'

# Tauri CLI: accept either a global install or the package.json dev dependency.
$hasGlobalTauri = $null -ne (Get-Command 'tauri' -ErrorAction SilentlyContinue)
$localTauri     = Join-Path $RepoRoot 'node_modules\.bin\tauri.cmd'
if ($hasGlobalTauri) {
    Write-Ok "Tauri CLI found (global)"
} elseif (Test-Path $localTauri) {
    Write-Ok "Tauri CLI found (local dev dependency)"
} else {
    Write-Ok "Tauri CLI not found yet; expecting it via 'npm install' (dev dependency) or 'npm i -g @tauri-apps/cli'"
}

# --- 2. Verify the engine rootfs tar ------------------------------------------
Write-Step "Verifying bundled engine rootfs"
if (-not (Test-Path $TarPath)) {
    Write-Host ""
    Write-Host "The Docker Engine rootfs tar is missing:" -ForegroundColor Red
    Write-Host "    $TarPath" -ForegroundColor Red
    Write-Host ""
    Write-Host "Build it first on a Linux/WSL host that has Docker, then re-run this script:" -ForegroundColor Yellow
    Write-Host "    # inside WSL/Linux, at the repository root:" -ForegroundColor Yellow
    Write-Host "    bash engine/build-rootfs.sh" -ForegroundColor Yellow
    Write-Host ""
    Write-Host "It must produce: src-tauri/resources/litedock-engine.tar" -ForegroundColor Yellow
    Fail "Engine rootfs tar not found. See instructions above."
}
$tarSizeMB = [math]::Round((Get-Item $TarPath).Length / 1MB, 1)
Write-Ok "Found litedock-engine.tar ($tarSizeMB MB)"

# --- 3. Install frontend dependencies -----------------------------------------
Write-Step "Installing frontend dependencies (npm install)"
Push-Location $RepoRoot
try {
    npm install
    if ($LASTEXITCODE -ne 0) { Fail "'npm install' failed (exit code $LASTEXITCODE)." }
    Write-Ok "Dependencies installed"

    # --- 4. Build the installer ------------------------------------------------
    Write-Step "Building installer (npm run tauri build) — first build is slow"
    npm run tauri build
    if ($LASTEXITCODE -ne 0) { Fail "'npm run tauri build' failed (exit code $LASTEXITCODE)." }
    Write-Ok "Build complete"
}
finally {
    Pop-Location
}

# --- 5. Report the produced installer -----------------------------------------
Write-Step "Locating installer"
$installer = $null
if (Test-Path $BundleDir) {
    $installer = Get-ChildItem -Path $BundleDir -Filter '*-setup.exe' -ErrorAction SilentlyContinue |
                 Sort-Object LastWriteTime -Descending |
                 Select-Object -First 1
    if (-not $installer) {
        $installer = Get-ChildItem -Path $BundleDir -Filter '*.exe' -ErrorAction SilentlyContinue |
                     Sort-Object LastWriteTime -Descending |
                     Select-Object -First 1
    }
}

Write-Host ""
if ($installer) {
    $sizeMB = [math]::Round($installer.Length / 1MB, 1)
    Write-Host "SUCCESS" -ForegroundColor Green
    Write-Host "Installer: $($installer.FullName)" -ForegroundColor Green
    Write-Host "Size:      $sizeMB MB" -ForegroundColor Green
} else {
    Write-Host "Build reported success, but no .exe was found under:" -ForegroundColor Yellow
    Write-Host "    $BundleDir" -ForegroundColor Yellow
    Write-Host "Check the Tauri bundler output above." -ForegroundColor Yellow
}
Write-Host ""
