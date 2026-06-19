# Building LiteDock from source

This guide takes a **clean Windows machine** from source to a signed-or-unsigned `.exe` installer. Follow it top to bottom; every command is copy-pasteable. The end product is an NSIS installer at:

```
src-tauri\target\release\bundle\nsis\LiteDock_<version>_x64-setup.exe
```

LiteDock is a Tauri 2 app: a Rust backend, a Svelte 5 + TypeScript + Vite frontend, and a bundled Docker Engine rootfs that ships as `src-tauri\resources\litedock-engine.tar`. You build the rootfs once on a Linux/Docker host, then build the installer on Windows.

## 1. Prerequisites

Install these on the Windows build machine unless noted otherwise.

- **Rust (stable) with the MSVC toolchain.** Install via [rustup](https://rustup.rs/). On Windows, rustup defaults to the `x86_64-pc-windows-msvc` toolchain, which is what Tauri needs. Verify:
  ```powershell
  rustup default stable
  rustc --version
  cargo --version
  ```
  The MSVC toolchain requires the **Microsoft C++ Build Tools** (the "Desktop development with C++" workload from the Visual Studio Build Tools installer), which provides the `link.exe` linker and the Windows SDK. Install it if `cargo build` later fails with a linker error.

- **Node.js LTS** (which includes npm). Verify:
  ```powershell
  node --version
  npm --version
  ```

- **Tauri CLI.** You can rely on the dev dependency declared in `package.json` (invoked via `npm run tauri ...`), or install it globally:
  ```powershell
  npm i -g @tauri-apps/cli
  ```
  This guide uses the `npm run tauri ...` form, which works with either.

- **WebView2 runtime.** Required at runtime, not build time. It is preinstalled on current Windows 10/11. If a target machine lacks it, the NSIS installer can provision it; for local dev just ensure "Microsoft Edge WebView2 Runtime" is present.

- **A Docker host to build the engine tar.** The rootfs is built on **Linux** (it assembles a minimal Alpine image with Docker Engine and friends). The simplest option on Windows is an **existing WSL Ubuntu** distro with Docker available, or any Linux box / CI runner with Docker. You do not need LiteDock itself installed to do this.

> Note on architecture: builds are **x64 only**. Use an x64 toolchain and an x64 rootfs.

## 2. Build the engine rootfs tar (run on Linux / WSL)

The installer embeds the engine as `src-tauri\resources\litedock-engine.tar`. Build it from a Linux shell that has Docker. If you are using WSL Ubuntu, open it and `cd` to the repository (your Windows checkout is reachable under `/mnt/<drive>/...`).

```bash
# Inside WSL/Linux, at the repository root:
bash engine/build-rootfs.sh
```

`engine/build-rootfs.sh` produces the rootfs tarball and writes it to `src-tauri/resources/litedock-engine.tar` (the same path Windows sees as `src-tauri\resources\litedock-engine.tar`). Confirm it exists before continuing:

```bash
ls -lh src-tauri/resources/litedock-engine.tar
```

If you built the tar on a separate Linux machine or CI runner, copy the resulting `litedock-engine.tar` into `src-tauri\resources\` on the Windows build machine.

## 3. Install frontend dependencies (Windows)

From the repository root in PowerShell:

```powershell
npm install
```

This installs the Svelte/Vite frontend and the Tauri CLI dev dependency.

## 4. Build the installer (Windows)

```powershell
npm run tauri build
```

This compiles the Rust backend in release mode, builds the Vite frontend, embeds the resources (including `litedock-engine.tar`), and runs Tauri's NSIS bundler. When it finishes, the installer is here:

```
src-tauri\target\release\bundle\nsis\LiteDock_<version>_x64-setup.exe
```

The first release build is slow because Rust compiles all dependencies from scratch; subsequent builds are incremental and much faster.

## Development workflow

For day-to-day development with hot reload:

```powershell
npm run tauri dev
```

This runs the Vite dev server and launches the Tauri app pointed at it, rebuilding the frontend on save and the Rust backend on change. The engine tar still needs to exist if you exercise code paths that import/start the distro; for pure UI work it is not required.

Convenience scripts are provided at the repo root:

- `.\scripts\dev.ps1` — runs `npm install` (only if `node_modules` is missing) then `npm run tauri dev`.
- `.\scripts\build-all.ps1` — checks prerequisites, verifies the engine tar exists, runs `npm install` then `npm run tauri build`, and prints the path to the produced `.exe`.

## Troubleshooting

- **`link.exe` not found / linker errors (MSVC).** The Visual C++ Build Tools are missing or incomplete. Install the "Desktop development with C++" workload from the Visual Studio Build Tools, then re-run the build in a fresh terminal so the environment is picked up.
- **WebView2 errors at launch ("WebView2 runtime not found").** Install the Microsoft Edge WebView2 Runtime (Evergreen) and relaunch. This affects running the app, not building it.
- **`litedock-engine.tar` not found during build.** You skipped or have not yet run step 2. Build the tar on Linux/WSL with `bash engine/build-rootfs.sh` and ensure it lands at `src-tauri\resources\litedock-engine.tar`, then rebuild.
- **Icon errors during bundling.** Tauri needs a complete icon set. Regenerate it from a single source PNG:
  ```powershell
  npm run tauri icon path\to\source-icon.png
  ```
  Then rebuild.
- **Node/npm not found, or wrong Rust toolchain.** Reconfirm the prerequisites in section 1. On Windows, ensure rustup's default is the MSVC (`...-pc-windows-msvc`) toolchain, not GNU.

## Code signing

Code signing the installer is **optional and out of scope** for this guide. Unsigned builds run fine but will show a Windows SmartScreen prompt. To sign, supply a code-signing certificate to Tauri's bundler configuration; consult the Tauri signing documentation. None of the steps above require a certificate.
