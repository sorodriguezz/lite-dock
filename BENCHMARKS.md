# LiteDock vs Docker Desktop — Benchmarks

This document defines a **rigorous, reproducible** methodology for comparing LiteDock against Docker Desktop on the **same machine**, plus result **templates** with placeholder cells for you to fill in. The goal is to measure what matters for a lightweight container runtime: installer size, idle RAM, RAM when the tool is closed, cold-start time until a container can run, idle CPU, and disk footprint.

A helper script, `scripts\measure-benchmarks.ps1`, automates collection of the runtime metrics and prints a summary you can paste below.

## Why the comparison is fair

- **Same hardware, same Windows build, same WSL2.** Run every measurement on one physical machine without changing hardware, power profile, or Windows version between runs.
- **One tool active at a time.** Docker Desktop and LiteDock both run a Docker Engine in WSL2 and both expose a local Docker socket. Running them simultaneously contends for WSL2 and skews RAM/CPU. Measure each tool with the other fully closed (and its WSL distro terminated).
- **Settled state.** After starting or stopping a tool, wait for the system to settle (no active pulls/builds, CPU idle) before sampling. Take **three samples** of each runtime metric and record the median.
- **Cold means cold.** Cold-start measurements begin from a state where the tool's WSL distro is terminated (verify with `wsl --list --verbose` — the distro should show `Stopped`).

## Definitions

- **UI process RAM** — working set of the desktop UI process(es). For LiteDock this is the Tauri app process plus its WebView2 child processes (`msedgewebview2`). For Docker Desktop this is `Docker Desktop.exe` and its helpers.
- **Engine RAM** — memory held by the WSL2 virtual machine that hosts the engine. On Windows this surfaces as the **`vmmemWSL`** process (older builds: `vmmem`). This is the single most important number for "idle weight," because it includes the Linux kernel + dockerd + containerd inside WSL2.
- **Total RAM** — UI process RAM + engine (`vmmemWSL`) RAM.
- **Closed/terminated RAM** — Total RAM after the tool is closed and its WSL distro is terminated. LiteDock targets **~0** here because it runs `wsl --terminate litedock-engine` on close.

> `vmmemWSL` is shared by all running WSL2 distros. To attribute it to a single tool, ensure only that tool's distro is running when you sample (the other tool closed and terminated).

## Environment to record (fill in once)

| Field | Value |
| --- | --- |
| CPU | _e.g. Intel Core i7-1165G7_ |
| Total system RAM | _GB_ |
| Disk | _NVMe / SATA SSD_ |
| Windows edition | _Windows 11 Pro_ |
| Windows build | _e.g. 22631_ |
| WSL version | _output of `wsl --version`_ |
| LiteDock version | _e.g. 1.0.0_ |
| Docker Desktop version | _e.g. 4.x.x_ |
| Date of measurement | _YYYY-MM-DD_ |

## Measurement commands

All commands are read-only and require **no administrator rights**. Use PowerShell 5.1+ or pwsh 7. Run them one tool at a time as described.

### Installer size

Measure the size of each downloaded installer file.

```powershell
# Point these at the actual installer files you downloaded / built.
$litedock = "src-tauri\target\release\bundle\nsis\LiteDock_1.0.0_x64-setup.exe"
$dockerDesktop = "$HOME\Downloads\Docker Desktop Installer.exe"

"{0,-18} {1,8:N1} MB" -f "LiteDock:",       ((Get-Item $litedock).Length / 1MB)
"{0,-18} {1,8:N1} MB" -f "Docker Desktop:",  ((Get-Item $dockerDesktop).Length / 1MB)
```

### Idle RAM (UI only, engine only, total)

With the tool **running and idle** (engine started, no containers doing work):

```powershell
# UI process RAM (working set). Adjust process names per tool.
# LiteDock UI + its WebView2 children:
Get-Process litedock, msedgewebview2 -ErrorAction SilentlyContinue |
  Measure-Object WorkingSet64 -Sum |
  ForEach-Object { "UI RAM: {0:N1} MB" -f ($_.Sum / 1MB) }

# Docker Desktop UI:
Get-Process "Docker Desktop", "com.docker.*", "Docker Desktop *" -ErrorAction SilentlyContinue |
  Measure-Object WorkingSet64 -Sum |
  ForEach-Object { "UI RAM: {0:N1} MB" -f ($_.Sum / 1MB) }

# Engine RAM = the WSL2 VM (vmmemWSL on current Windows, vmmem on older):
Get-Process vmmemWSL, vmmem -ErrorAction SilentlyContinue |
  Measure-Object WorkingSet64 -Sum |
  ForEach-Object { "Engine (WSL VM) RAM: {0:N1} MB" -f ($_.Sum / 1MB) }
```

Total RAM = UI RAM + Engine RAM. Take three samples a few seconds apart and record the median.

### RAM with the app closed / WSL terminated (~0 target)

Close the tool. For LiteDock this terminates its distro automatically; for Docker Desktop you may need "Quit Docker Desktop." Then confirm the distro is stopped and re-sample the WSL VM:

```powershell
wsl --list --verbose          # the tool's distro should read STATE = Stopped
Get-Process vmmemWSL, vmmem -ErrorAction SilentlyContinue |
  Measure-Object WorkingSet64 -Sum |
  ForEach-Object { "Engine RAM after close: {0:N1} MB" -f ($_.Sum / 1MB) }
```

If `vmmemWSL` is gone entirely (no WSL2 distros running), engine RAM after close is effectively 0.

To force-terminate LiteDock's distro for a clean cold-start baseline:

```powershell
wsl --terminate litedock-engine
```

### Cold-start time until a container can run

Measure wall-clock time from "engine not running" to "a container has actually run." Start from a terminated distro. The most objective definition is the time for `docker run` to complete a tiny container once the engine is reachable; include the time to bring the engine up.

```powershell
# Ensure a cold start:
wsl --terminate litedock-engine   # (for Docker Desktop, fully quit it first instead)

# Then time: start the tool, wait until its engine answers, run hello-world.
# Replace <client-docker> with the docker client the tool exposes, or run inside the distro.
Measure-Command {
    # 1. Launch the tool (or let your lifecycle code start the engine), e.g.:
    #    Start-Process "C:\Program Files\LiteDock\LiteDock.exe"
    # 2. Poll until the Engine API answers, then run a container:
    do {
        Start-Sleep -Milliseconds 500
        $ok = $false
        try { Invoke-RestMethod "http://127.0.0.1:23750/_ping" -TimeoutSec 2 | Out-Null; $ok = $true } catch {}
    } until ($ok)
    docker run --rm hello-world | Out-Null
}
```

`Measure-Command` returns a `TimeSpan`; record `TotalSeconds`. For LiteDock the engine answers at `http://127.0.0.1:23750`. For Docker Desktop, poll its Docker socket / `docker info` instead, and start timing from the moment you launch the app. Run three cold starts and record the median; note whether each was the very first start after boot (slower) or a warm-OS-cache start.

### Idle CPU

With the tool running and idle, sample CPU over a short window. Two approaches:

```powershell
# Per-process CPU seconds delta over ~10s, converted to % of one core:
$names = "vmmemWSL","vmmem","litedock","msedgewebview2","Docker Desktop"
$before = Get-Process $names -ErrorAction SilentlyContinue |
          Select-Object Name, @{n='CPU';e={$_.CPU}}
Start-Sleep -Seconds 10
$after  = Get-Process $names -ErrorAction SilentlyContinue |
          Select-Object Name, @{n='CPU';e={$_.CPU}}
# %CPU (one core) = delta CPU-seconds / elapsed-seconds * 100, summed across the processes.
```

Or read the system-wide processor queue / total via CIM as a sanity check:

```powershell
(Get-CimInstance Win32_Processor | Measure-Object LoadPercentage -Average).Average
```

In **Task Manager**, idle CPU for the tool is visible against the `Vmmem`/`vmmemWSL` entry and the app's processes on the Details tab; use it as a cross-check, not the primary figure.

### Disk footprint

Measure on-disk size of (a) the WSL2 distro VHD and (b) the tool's installed program files.

```powershell
# LiteDock engine VHD + data:
$litedockData = "$env:LOCALAPPDATA\LiteDock"
"{0,-26} {1,8:N1} MB" -f "LiteDock data dir:",
  ((Get-ChildItem $litedockData -Recurse -File -ErrorAction SilentlyContinue |
    Measure-Object Length -Sum).Sum / 1MB)

# Docker Desktop data (typical locations; adjust if customized):
$dd = "$env:LOCALAPPDATA\Docker", "$env:APPDATA\Docker", "$env:APPDATA\Docker Desktop"
foreach ($p in $dd) {
  if (Test-Path $p) {
    "{0,-26} {1,8:N1} MB" -f ("$p :"),
      ((Get-ChildItem $p -Recurse -File -ErrorAction SilentlyContinue |
        Measure-Object Length -Sum).Sum / 1MB)
  }
}
```

For an apples-to-apples disk comparison, take both measurements after a **fresh install with the same one image pulled** (e.g. only `hello-world`), so neither tool's number is inflated by accumulated images.

## Results template

Fill in the cells. Record the **median of three samples** for runtime metrics. Keep one decimal.

### Size and install footprint

| Metric | LiteDock | Docker Desktop |
| --- | --- | --- |
| Installer size (MB) | _____ | _____ |
| Installed disk footprint, fresh + only `hello-world` (MB) | _____ | _____ |

### Idle RAM (tool running, engine up, idle)

| Metric | LiteDock | Docker Desktop |
| --- | --- | --- |
| UI process RAM (MB) | _____ | _____ |
| Engine RAM — `vmmemWSL` (MB) | _____ | _____ |
| **Total idle RAM (MB)** | _____ | _____ |

### RAM with tool closed / WSL terminated

| Metric | LiteDock | Docker Desktop |
| --- | --- | --- |
| Distro state after close (`wsl -l -v`) | _____ | _____ |
| Engine RAM after close (MB, ~0 target) | _____ | _____ |
| Residual UI/background RAM (MB) | _____ | _____ |

### Cold start and idle CPU

| Metric | LiteDock | Docker Desktop |
| --- | --- | --- |
| Cold start to first `docker run` complete (s) | _____ | _____ |
| Idle CPU, % of one core over 10 s | _____ | _____ |

## Expected direction of results

LiteDock is expected to **win clearly on idle RAM when closed and on idle footprint**, because it terminates its WSL2 distro on app close (`wsl --terminate litedock-engine`), so `vmmemWSL` and the Linux kernel/dockerd memory go to roughly zero when you are not using it. Docker Desktop keeps background services and (typically) its WSL distro resident, so its closed-state and idle RAM remain substantial. LiteDock's installer and UI process RAM are also expected to be smaller, because the desktop shell is a Tauri app using the OS WebView2 rather than a bundled Chromium. Cold-start time is the metric where the gap should be **smallest** and could go either way: both tools ultimately start the same kind of Docker Engine in WSL2, and LiteDock pays a deliberate cold-start cost (bringing the terminated distro up) in exchange for its near-zero idle footprint. Engine-up idle RAM and idle CPU should be comparable when both engines are actually running, since both host a genuine dockerd.
