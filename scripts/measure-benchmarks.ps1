#Requires -Version 5.1
<#
.SYNOPSIS
    Collect LiteDock vs Docker Desktop benchmark metrics into a printed report.

.DESCRIPTION
    Read-only. No administrator rights required. Samples:
      - UI process RAM (LiteDock app + WebView2; Docker Desktop processes)
      - Engine RAM via the WSL2 VM process (vmmemWSL / vmmem)
      - WSL distro list with state (wsl --list --verbose)
      - Idle CPU as % of one core, sampled over a short window
      - LiteDock on-disk data footprint (%LOCALAPPDATA%\LiteDock)

    Run the tool you want to measure FIRST (engine up, idle), with the other
    tool closed and its distro terminated, then run this script. Paste the
    printed summary into BENCHMARKS.md.

.PARAMETER SampleSeconds
    Length of the CPU sampling window. Default 10.

.EXAMPLE
    .\scripts\measure-benchmarks.ps1
.EXAMPLE
    .\scripts\measure-benchmarks.ps1 -SampleSeconds 15
#>

[CmdletBinding()]
param(
    [ValidateRange(2, 120)]
    [int]$SampleSeconds = 10
)

$ErrorActionPreference = 'Stop'

# Process name groups (without .exe; Get-Process matches base names, wildcards allowed).
$LiteDockUiNames     = @('litedock', 'msedgewebview2')
$DockerDesktopNames  = @('Docker Desktop', 'com.docker.*', 'Docker Desktop *', 'frontend')
$WslVmNames          = @('vmmemWSL', 'vmmem')

function Write-Section { param([string]$Title) Write-Host ""; Write-Host "=== $Title ===" -ForegroundColor Cyan }
function Get-WsBytes {
    param([string[]]$Names)
    $procs = Get-Process -Name $Names -ErrorAction SilentlyContinue
    if (-not $procs) { return [pscustomobject]@{ Bytes = 0L; Count = 0 } }
    $sum = ($procs | Measure-Object WorkingSet64 -Sum).Sum
    [pscustomobject]@{ Bytes = [int64]$sum; Count = $procs.Count }
}
function Format-MB { param([int64]$Bytes) ("{0:N1} MB" -f ($Bytes / 1MB)) }

Write-Host ""
Write-Host "LiteDock benchmark collector" -ForegroundColor White
Write-Host ("Timestamp: {0}" -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'))
Write-Host "Tip: run the tool under test first (engine up, idle), with the other tool closed and terminated."

# --- Environment ---------------------------------------------------------------
Write-Section "Environment"
$os  = Get-CimInstance Win32_OperatingSystem
$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
Write-Host ("OS:           {0} (build {1})" -f $os.Caption, $os.BuildNumber)
Write-Host ("CPU:          {0}" -f $cpu.Name.Trim())
Write-Host ("Logical CPUs: {0}" -f $env:NUMBER_OF_PROCESSORS)
Write-Host ("Total RAM:    {0:N1} GB" -f ($os.TotalVisibleMemorySize / 1MB))
$wslVer = (& wsl.exe --version 2>$null | Out-String).Trim()
if ($wslVer) { Write-Host "WSL version:"; Write-Host $wslVer } else { Write-Host "WSL version:  (wsl --version unavailable)" }

# --- WSL distro list -----------------------------------------------------------
Write-Section "WSL distros (wsl --list --verbose)"
try {
    $wslList = (& wsl.exe --list --verbose 2>&1 | Out-String)
    if ($wslList.Trim()) { Write-Host $wslList.Trim() } else { Write-Host "(no output)" }
    if ($wslList -match 'litedock-engine') {
        Write-Host "litedock-engine distro: present" -ForegroundColor Green
    } else {
        Write-Host "litedock-engine distro: not found (LiteDock may not be installed/imported yet)" -ForegroundColor Yellow
    }
}
catch {
    Write-Host "Could not run 'wsl --list --verbose': $($_.Exception.Message)" -ForegroundColor Yellow
}

# --- RAM snapshot --------------------------------------------------------------
Write-Section "RAM snapshot (working set)"
$liteUi   = Get-WsBytes -Names $LiteDockUiNames
$ddUi     = Get-WsBytes -Names $DockerDesktopNames
$engine   = Get-WsBytes -Names $WslVmNames

Write-Host ("LiteDock UI (app + WebView2): {0}  [{1} process(es)]" -f (Format-MB $liteUi.Bytes), $liteUi.Count)
Write-Host ("Docker Desktop UI:           {0}  [{1} process(es)]" -f (Format-MB $ddUi.Bytes),   $ddUi.Count)
Write-Host ("Engine — WSL2 VM (vmmemWSL): {0}  [{1} process(es)]" -f (Format-MB $engine.Bytes), $engine.Count)

if ($engine.Count -eq 0) {
    Write-Host "  -> No WSL2 VM process running: engine RAM is effectively 0 (a tool is closed / distro terminated)." -ForegroundColor Green
}

# Totals (per tool = that tool's UI + the shared WSL VM, valid only if only that tool's distro runs).
$liteTotal = $liteUi.Bytes + $engine.Bytes
$ddTotal   = $ddUi.Bytes   + $engine.Bytes
Write-Host ""
Write-Host "Per-tool total (UI + WSL VM). NOTE: vmmemWSL is shared by all running"
Write-Host "WSL2 distros; this total is only meaningful for the tool whose distro"
Write-Host "is the only one running right now."
Write-Host ("  LiteDock total (if its distro is the running one):       {0}" -f (Format-MB $liteTotal))
Write-Host ("  Docker Desktop total (if its distro is the running one): {0}" -f (Format-MB $ddTotal))

# --- Idle CPU over a window ----------------------------------------------------
Write-Section ("Idle CPU over {0}s (% of one core)" -f $SampleSeconds)
$cpuNames = $LiteDockUiNames + $DockerDesktopNames + $WslVmNames

function Get-CpuSecondsByName {
    param([string[]]$Names)
    $map = @{}
    foreach ($p in (Get-Process -Name $Names -ErrorAction SilentlyContinue)) {
        $key = $p.ProcessName
        if (-not $map.ContainsKey($key)) { $map[$key] = 0.0 }
        # $p.CPU is total processor time (seconds) used by the process so far.
        if ($null -ne $p.CPU) { $map[$key] += [double]$p.CPU }
    }
    $map
}

$before = Get-CpuSecondsByName -Names $cpuNames
$swStart = Get-Date
Start-Sleep -Seconds $SampleSeconds
$after = Get-CpuSecondsByName -Names $cpuNames
$elapsed = ((Get-Date) - $swStart).TotalSeconds
if ($elapsed -le 0) { $elapsed = $SampleSeconds }

$rows = @()
$totalPct = 0.0
foreach ($name in ($after.Keys | Sort-Object)) {
    $delta = [double]$after[$name]
    if ($before.ContainsKey($name)) { $delta -= [double]$before[$name] }
    if ($delta -lt 0) { $delta = 0 }   # process restarted within the window
    $pct = ($delta / $elapsed) * 100.0
    $totalPct += $pct
    $rows += [pscustomobject]@{ Process = $name; 'CPU % (1 core)' = [math]::Round($pct, 1) }
}
if ($rows.Count -gt 0) {
    $rows | Format-Table -AutoSize | Out-String | Write-Host
} else {
    Write-Host "(none of the watched processes were running during the sample)"
}
Write-Host ("Combined watched-process CPU: {0:N1} % of one core" -f $totalPct)
$sysLoad = (Get-CimInstance Win32_Processor | Measure-Object LoadPercentage -Average).Average
Write-Host ("System-wide CPU load (sanity check):                {0} %" -f $sysLoad)

# --- Disk footprint ------------------------------------------------------------
Write-Section "Disk footprint"
$liteData = Join-Path $env:LOCALAPPDATA 'LiteDock'
if (Test-Path $liteData) {
    $bytes = (Get-ChildItem $liteData -Recurse -File -ErrorAction SilentlyContinue |
              Measure-Object Length -Sum).Sum
    if (-not $bytes) { $bytes = 0 }
    Write-Host ("LiteDock data dir ({0}): {1}" -f $liteData, (Format-MB ([int64]$bytes)))
} else {
    Write-Host ("LiteDock data dir not found ({0}). LiteDock may not be installed yet." -f $liteData) -ForegroundColor Yellow
}
foreach ($p in @((Join-Path $env:LOCALAPPDATA 'Docker'), (Join-Path $env:APPDATA 'Docker'), (Join-Path $env:APPDATA 'Docker Desktop'))) {
    if (Test-Path $p) {
        $b = (Get-ChildItem $p -Recurse -File -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum
        if (-not $b) { $b = 0 }
        Write-Host ("Docker Desktop data ({0}): {1}" -f $p, (Format-MB ([int64]$b)))
    }
}

# --- Paste-ready summary -------------------------------------------------------
Write-Section "Paste-ready summary"
Write-Host "Copy the lines below into BENCHMARKS.md (fill the other tool's column by re-running with it active):"
Write-Host ""
Write-Host ("  Timestamp:                 {0}" -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'))
Write-Host ("  LiteDock UI RAM:           {0}" -f (Format-MB $liteUi.Bytes))
Write-Host ("  Docker Desktop UI RAM:     {0}" -f (Format-MB $ddUi.Bytes))
Write-Host ("  Engine (WSL VM) RAM:       {0}" -f (Format-MB $engine.Bytes))
Write-Host ("  LiteDock total (UI+WSL):   {0}" -f (Format-MB $liteTotal))
Write-Host ("  Combined watched CPU:      {0:N1} % of one core over {1}s" -f $totalPct, $SampleSeconds)
Write-Host ""
Write-Host "Reminder: take three samples and record the median; sample each tool"
Write-Host "with the other fully closed and its WSL distro terminated."
Write-Host ""
