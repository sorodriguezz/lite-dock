# LiteDock

LiteDock is a lightweight, all-in-one alternative to Docker Desktop for Windows. It gives you a real Docker Engine and a clean desktop UI for managing containers, images, volumes, networks, builds, and Compose projects — without the weight, the manual WSL setup, or the licensing cost.

LiteDock is built around one idea: the desktop app is a **thin client of the real Docker Engine**. It does not reimplement container logic. Every image, container, volume, network, build, and Compose operation is executed by a genuine, unmodified Docker Engine (Moby) that LiteDock installs and runs for you inside a dedicated, headless WSL2 distro. The UI is just a comfortable window onto that engine.

## Why LiteDock

- **Lighter.** The desktop shell is a Tauri 2 app that uses the OS WebView2 runtime instead of bundling a full Chromium. The installer is a few MB and idle RAM is small. Closing the window minimizes LiteDock to the system tray and keeps your containers running in the background; choosing **Salir** (Quit) from the tray stops the WSL2 distro so idle resource use drops to roughly zero. You can also cap the engine's RAM from **Configuración**. Docker Desktop keeps background services resident.
- **All-in-one.** Engine, containerd, runc, BuildKit, Docker CLI, and Compose v2 are bundled inside a minimal Alpine rootfs. There is nothing else to download.
- **No manual WSL setup.** A first-run wizard detects whether WSL2 is already installed and uses it if so. It only enables WSL2 when it is actually missing, guides you through a single reboot if Windows requires one, and imports a dedicated distro that is kept separate from your own.
- **No licensing cost.** LiteDock ships only Apache-2.0 and permissively licensed components. It does **not** bundle Docker Desktop or anything under Docker's commercial license. See [docs/LICENSES.md](docs/LICENSES.md).

## Features

- Containers: list, start, stop, restart, remove, inspect, and view stats.
- Images: list, pull, tag, remove, prune.
- Volumes and networks: create, list, inspect, remove.
- **Build** with BuildKit (`docker build`/buildx), streamed live to the UI.
- **Compose v2**: bring projects up and down, streamed live to the UI.
- Streaming **logs** and interactive **exec** into running containers.
- A dashboard with engine status and a one-click start/stop of the backend.

Under the hood, classic operations (containers, images, volumes, networks, logs, stats, events) go through the Docker Engine REST API via the Rust [bollard](https://crates.io/crates/bollard) crate. `docker build` (BuildKit) and `docker compose` are run as the real `docker` CLI inside the distro and their output is streamed back — a deliberate hybrid explained in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Screenshots

> _Screenshots placeholder — add dashboard, containers, build, and Compose views here._
>
> `docs/images/dashboard.png` · `docs/images/containers.png` · `docs/images/build.png`

## Quick start (use)

1. Download the LiteDock installer (`LiteDock_x64-setup.exe`) and run it. Administrator rights are not required to run the app, though enabling WSL2 for the first time may prompt for elevation.
2. Launch LiteDock. On first run, the setup wizard will:
   - detect whether WSL2 is already installed and **skip reinstalling it if present**;
   - check that virtualization is available and enable WSL2 only if it is missing (it will ask you to reboot once if Windows requires it);
   - import the dedicated `litedock-engine` distro from the bundled rootfs (only if it is not already imported);
   - start the Docker Engine and verify it by running a `hello-world` container.
3. Once the wizard reports the engine is healthy, use the UI to pull images, run containers, build, and bring Compose projects up.
4. Closing the window keeps LiteDock running in the **system tray**, so your containers keep running in the background. Right-click the tray icon for **Mostrar / Iniciar / Detener / Reiniciar / Salir** — choose **Salir** to stop the engine and free its RAM.

The Docker Engine is reachable from Windows at `http://127.0.0.1:23750` (bound to localhost only). Inside the distro, dockerd also listens on the usual `unix:///var/run/docker.sock`.

**Requirements:** Windows 10 version 2004 (build 19041) or later, or Windows 11. x64 only. WebView2 runtime (preinstalled on current Windows; the installer can provision it if absent).

## Build and run locally

Building LiteDock produces the NSIS `.exe` installer. There are three pieces: the **engine rootfs** (a Linux tarball, built once on any Linux/WSL/Docker host) and the optional **bundled Docker CLI**, both of which become app resources, and finally the **Tauri app** itself (built on Windows). This section is self-contained; the deeper reference lives in [docs/BUILD.md](docs/BUILD.md).

### Prerequisites

- **Windows 10** version 2004 (build 19041) or later, or **Windows 11**, **x64**.
- **Rust** (stable, MSVC toolchain) — install via [rustup.rs](https://rustup.rs/).
- **Node.js LTS** + npm — [nodejs.org](https://nodejs.org/).
- **WSL2** with a Linux distro (e.g. Ubuntu from the Microsoft Store) — needed only to *build* the engine rootfs. At runtime the app manages its own dedicated distro.
- **WebView2 runtime** — preinstalled on current Windows.

> Tip: clone the repo to a normal Windows path (e.g. `C:\dev\litedock`) and run the Windows steps from **PowerShell** at the project root.

### Step 1 — Build the engine rootfs (`litedock-engine.tar`)

This is required: the installer bundles it. It produces `src-tauri/resources/litedock-engine.tar`. Pick the path that matches your machine:

**Option A — with Docker** (Docker Desktop, or `docker` inside WSL/Linux/macOS):

```bash
# from the repo root, in a shell that can reach a Docker daemon:
bash engine/build-rootfs.sh
```

**Option B — without Docker, inside WSL** (the common case):

```bash
# open your WSL distro (e.g. `wsl`), cd into the repo under /mnt/<drive>/...,
# then run (it re-execs with sudo as needed). Do NOT run this in Git Bash/MSYS:
bash engine/build-rootfs-wsl.sh
```

Either way you end up with `src-tauri\resources\litedock-engine.tar` (~250–350 MB).

### Step 2 — Bundle the Docker CLI (optional, recommended)

This makes the installer "all-in-one" so the app can wire `docker` / `docker compose` into your terminals (the *Habilitar comandos docker* button). From **PowerShell** at the repo root:

```powershell
.\scripts\fetch-docker-cli.ps1
# -> src-tauri\resources\docker-cli\{docker.exe, docker-compose.exe}
```

If you skip this, the build still works; it just omits that feature.

### Step 3 — Build the installer

The one-shot script checks prerequisites, verifies the rootfs tar exists, installs frontend deps, and runs the Tauri build:

```powershell
.\scripts\build-all.ps1
```

…or do it manually:

```powershell
npm install
npm run tauri build
```

The installer lands at:

```
src-tauri\target\release\bundle\nsis\LiteDock_<version>_x64-setup.exe
```

### Run in development (hot reload)

For day-to-day work, run the app with live reload (Vite + Tauri). The engine rootfs from Step 1 must be present so the first-run wizard can import the distro:

```powershell
.\scripts\dev.ps1
# or: npm install && npm run tauri dev
```

### Don't want to build it yourself?

Every push to `main` that bumps the version in `package.json` triggers the GitHub Action in `.github/workflows/release.yml`, which builds the rootfs (Linux runner) + the installer (Windows runner) and publishes a tagged **GitHub Release** with the `.exe` attached — ready to download.

## Project status

LiteDock is a **research preview** developed in phases. It targets a focused use case and is not yet a drop-in replacement for every Docker Desktop feature. Expect rough edges; the architecture, trade-offs, and known risks are documented in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). arm64 support is future work; current builds are x64 only.

## License

LiteDock's own source is provided under its repository license. It redistributes Docker Engine and related components under Apache-2.0 and other permissive licenses, and bundles no commercially licensed Docker software. A full component-by-component breakdown and a compliance note are in [docs/LICENSES.md](docs/LICENSES.md).
