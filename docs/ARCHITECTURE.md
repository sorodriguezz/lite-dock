# LiteDock Architecture

This document describes how LiteDock is put together, the principle that the UI is a thin Docker API client, and the deliberate engineering trade-offs behind the design. It is written for developers who want to understand or extend the app.

## Canonical constants

These values are referenced throughout the codebase and the rest of the docs. They are authoritative.

| Thing | Value |
| --- | --- |
| Product name | LiteDock |
| Rust crate / npm package | `litedock` |
| Bundle identifier | `com.litedock.app` |
| WSL distro | `litedock-engine` |
| Engine endpoint from Windows | `http://127.0.0.1:23750` (localhost only) |
| Engine socket inside distro | `unix:///var/run/docker.sock` |
| Windows data dir | `%LOCALAPPDATA%\LiteDock` (VHD under `\engine`, logs under `\logs`) |
| Bundled rootfs asset | `src-tauri\resources\litedock-engine.tar` |
| Minimum OS | Windows 10 2004 / build 19041, or Windows 11; x64 only |

## Layered overview

```
+---------------------------------------------------------------+
|  Windows desktop                                              |
|                                                               |
|   +-------------------------------------------------------+   |
|   |  LiteDock app (Tauri 2)                               |   |
|   |                                                       |   |
|   |  Frontend: Svelte 5 + TypeScript + Vite               |   |
|   |  rendered in the OS WebView2 (no bundled Chromium)    |   |
|   |          |  Tauri IPC (commands / events)             |   |
|   |          v                                            |   |
|   |  Backend: Rust                                        |   |
|   |   - bollard  -> Docker Engine REST API                |   |
|   |     (containers, images, volumes, networks,           |   |
|   |      logs, stats, events, exec)                       |   |
|   |   - CLI streamer -> `docker build` / `docker compose` |   |
|   |     (spawned inside the distro, output streamed)      |   |
|   |   - lifecycle manager (start/stop dockerd, WSL)       |   |
|   +---------------------|------------------|--------------+   |
|                         |                  |                  |
|        TCP 127.0.0.1:23750         wsl.exe (exec/import/      |
|        (Engine REST API)            terminate)               |
|                         |                  |                  |
+-------------------------|------------------|------------------+
                          v                  v
+---------------------------------------------------------------+
|  WSL2 distro: litedock-engine  (minimal Alpine rootfs)        |
|                                                               |
|   dockerd  (Docker Engine / Moby)                             |
|     - listens on unix:///var/run/docker.sock                  |
|     - listens on tcp://127.0.0.1:23750 (forwarded to Windows) |
|   containerd  +  runc                                         |
|   BuildKit (buildx)                                           |
|   docker CLI  +  Compose v2 plugin                            |
|   data root on the distro's ext4 VHD (overlay2)               |
+---------------------------------------------------------------+
```

## Core principle: the UI is a thin Docker API client

LiteDock contains no container runtime logic of its own. It does not implement image layers, networking, build graphs, or Compose semantics. All of that is provided by a genuine, unmodified Docker Engine (Moby) plus containerd, runc, BuildKit, the Docker CLI, and the Compose v2 plugin, running inside the WSL2 distro.

The Rust backend's entire job is to (1) manage the lifecycle of that engine and the distro that hosts it, and (2) translate UI intents into Docker Engine API calls or `docker` CLI invocations and stream the results back to the Svelte frontend. This keeps the app small, keeps behavior identical to "real Docker," and means engine upgrades are mostly a matter of rebuilding the bundled rootfs.

## Why WSL2 and a dedicated distro

Docker Engine is Linux software. On Windows, the pragmatic way to run a real Linux kernel with good performance and tight OS integration is WSL2. LiteDock therefore runs dockerd inside WSL2 rather than shipping a full VM stack.

LiteDock installs its engine into a **dedicated, headless distro** named `litedock-engine`, kept entirely separate from any distros the user already has (Ubuntu, Debian, etc.), exactly as Docker Desktop isolates its `docker-desktop` distro. Reasons:

- **Isolation and reproducibility.** The engine gets a known, minimal Alpine userland with a pinned set of binaries. Nothing the user does in their own distros can break it, and vice versa.
- **Clean lifecycle.** LiteDock can `wsl --terminate litedock-engine` on app close without touching the user's distros.
- **Respect for existing setups.** If WSL2 is already enabled, LiteDock uses it and does not reinstall or reconfigure it. It only imports its own distro (and only if not already imported).

The distro is imported from the bundled tarball `src-tauri\resources\litedock-engine.tar`. The resulting ext4 VHD lives under `%LOCALAPPDATA%\LiteDock\engine`, and LiteDock's own logs under `%LOCALAPPDATA%\LiteDock\logs`.

## The bollard-vs-CLI hybrid

LiteDock talks to the engine in two complementary ways. This split is deliberate and worth understanding.

**Most operations go through the Docker Engine REST API via the [bollard](https://crates.io/crates/bollard) crate.** Containers, images, volumes, networks, log streaming, stats, events, and interactive exec all have stable, well-defined endpoints in the Engine API. Calling them directly from Rust is fast, typed, gives structured errors and streaming bodies, and avoids spawning a process per action.

**`docker build` (BuildKit) and `docker compose` v2 are run as the real `docker` CLI inside the distro, and their stdout/stderr are streamed back to the UI.** This is because:

- **Compose has no stable Engine API.** Compose v2 is a CLI plugin that orchestrates many Engine API calls (and depends on project files, profiles, env interpolation, and dependency ordering). There is no single supported `/compose` endpoint to call. Reimplementing Compose against the Engine API would mean re-creating a large, moving target — the opposite of the "thin client" principle. Running the official `docker compose` binary guarantees correct, up-to-date semantics.
- **BuildKit needs the buildx session.** Modern `docker build` uses BuildKit, which establishes a build session (for caching, secrets, SSH forwarding, multi-stage graph solving, and progress output) that is not a plain REST call you can comfortably drive from a generic client. The CLI/buildx already implements this protocol correctly, including the streaming progress UI we forward to the frontend.

So: structured, stable, high-frequency operations use bollard; the two areas where the CLI is the de-facto interface (Compose and BuildKit) are driven by streaming the CLI. The frontend sees a uniform stream of events either way.

## Transport: TCP on 127.0.0.1:23750

dockerd inside the distro listens on its Unix socket (`/var/run/docker.sock`) and additionally on `tcp://127.0.0.1:23750`. WSL2 forwards `127.0.0.1` between the distro and the Windows host, so the Rust backend reaches the engine at `http://127.0.0.1:23750`.

Why TCP rather than the Unix socket directly:

- It is the most reliable, well-supported way to reach a service running in WSL2 from a native Windows process, and bollard speaks HTTP over TCP cleanly.
- It avoids depending on Windows named-pipe ↔ Unix-socket bridging.

**Security rationale.** The listener is bound to `127.0.0.1` only — never `0.0.0.0`. The Docker Engine API grants control equivalent to root on the host it manages, so exposing it on a routable interface would be dangerous. Binding to loopback means only processes on the same machine can reach it. There is intentionally **no TLS** on this local socket (see Risks & trade-offs); the loopback binding is the security boundary, mirroring how local Docker sockets are conventionally trusted on a single-user developer machine.

## Lifecycle: lazy daemon, terminate on close

LiteDock optimizes for low idle cost.

**First run (setup wizard):**

1. Detect whether WSL2 is already installed. If it is, use it — do **not** reinstall.
2. Check that virtualization/hypervisor support is available.
3. If WSL2 is missing, enable it. If Windows requires a reboot to finish enabling the platform, guide the user through exactly one reboot and resume afterward.
4. Import the `litedock-engine` distro from `litedock-engine.tar` — **only if it is not already imported**.
5. Start dockerd and verify the engine by running a `hello-world` container.

**Normal run:**

- dockerd is started **lazily** — the distro is brought up and the daemon launched when the app needs the engine, not eagerly at boot.
- On app close, LiteDock runs `wsl --terminate litedock-engine`, which shuts the distro down. With the distro terminated, the engine and the WSL VM stop consuming memory, so idle RAM drops to roughly zero. This is the central reason LiteDock's idle footprint beats an always-resident service model.

## Path translation for build and Compose contexts

`docker build` and `docker compose` operate on a build context / project directory that the user expresses with Windows paths. Inside the distro those paths must be addressed through the WSL2 DrvFS mount of the Windows drives.

LiteDock translates Windows paths to their WSL2 equivalents before invoking the CLI: for example `C:\Users\me\project` becomes `/mnt/c/Users/me/project`. The general rule is `<Drive>:\<rest>` → `/mnt/<drive-lowercased>/<rest with backslashes turned into forward slashes>`. This lets users point builds and Compose projects at ordinary Windows folders while the engine reads them through `/mnt`.

## Risks & trade-offs

These are known, accepted limitations of the current design. They are documented here so they are not surprises.

- **overlay2 vs vfs storage driver.** Docker's `overlay2` storage driver requires overlayfs support and works correctly on the ext4 VHD that backs the distro; the engine is configured to use the distro's native ext4 data root so `overlay2` is available. If the engine ever fell back to the `vfs` driver (which copies whole layers instead of using overlays), disk usage and image operations would be dramatically slower and larger. The rootfs/data-root configuration is chosen specifically to keep `overlay2` working, and this is a thing to watch when changing where the data root lives.
- **localhost forwarding reliability.** Reaching the engine at `127.0.0.1:23750` relies on WSL2's loopback forwarding between the distro and Windows. This is generally reliable but has historically had edge cases across Windows/WSL versions (timing right after the distro starts, networking-mode changes). LiteDock starts the daemon lazily and verifies connectivity before reporting "healthy," and retries on transient connection failures.
- **DrvFS build performance.** Builds and Compose projects whose context lives on a Windows drive are read through `/mnt/<drive>` (DrvFS/drvfs), which is noticeably slower than the distro's native ext4 for large contexts with many files. This is the standard WSL2 cross-filesystem cost. Users who need maximum build speed can keep sources inside the Linux filesystem, but the default supports ordinary Windows paths for convenience.
- **No TLS on the local TCP socket.** The Engine API is served over plain HTTP on `127.0.0.1:23750`. There is no transport encryption or client-certificate auth on this socket. The mitigation is that it is bound to loopback only, so it is not reachable from other hosts; on a single-user developer machine this matches the conventional trust model for a local Docker socket. It is explicitly **not** suitable for exposing to a network, and LiteDock never binds it to a routable address.

## arm64 future work

Current builds are **x64 only**. Supporting Windows on arm64 is future work: it requires an arm64 Alpine rootfs with arm64 builds of dockerd/containerd/runc/BuildKit/Compose, an arm64 Rust/Tauri build of the app, and validation of WSL2 loopback forwarding and DrvFS behavior on arm64 hardware. The architecture does not preclude it — the engine is just another rootfs and the app is portable Rust + WebView2 — but it is out of scope for the research-preview phase.
