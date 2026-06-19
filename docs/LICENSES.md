# LiteDock — Redistributed components and licenses

LiteDock bundles and redistributes a number of third-party components. This document lists each one and its license. **All redistributed engine components are Apache-2.0 or permissively licensed.** LiteDock does **not** bundle Docker Desktop, and it does **not** include anything under Docker's commercial license.

## Summary

LiteDock ships a genuine, unmodified Docker Engine and its toolchain (containerd, runc, BuildKit, the Docker CLI, and Compose v2), all licensed under **Apache-2.0**, inside a minimal **Alpine** Linux rootfs (permissive: MIT/BSD-style, with BusyBox under GPL-2.0 aggregated as a separate program — see below). The desktop app is built with **Tauri** (MIT / Apache-2.0), talks to the engine via **bollard** (Apache-2.0), and renders a **Svelte** (MIT) frontend.

## Engine and container toolchain

| Component | Role | License |
| --- | --- | --- |
| Docker Engine (Moby / `dockerd`) | Container engine (REST API + daemon) | Apache-2.0 |
| containerd | Container runtime / supervisor | Apache-2.0 |
| runc | OCI low-level runtime | Apache-2.0 |
| BuildKit (buildx) | Build backend for `docker build` | Apache-2.0 |
| Docker Compose v2 | Compose CLI plugin | Apache-2.0 |
| Docker CLI (`docker`) | Command-line client run inside the distro | Apache-2.0 |

These are redistributed unmodified inside the `litedock-engine` rootfs. Their `NOTICE` and `LICENSE` files are retained in the rootfs as required by Apache-2.0.

## Base Linux userland (rootfs)

| Component | Role | License |
| --- | --- | --- |
| Alpine Linux base | Minimal Linux distribution providing the userland | MIT / BSD-style (permissive) |
| musl libc | C standard library | MIT |
| BusyBox | Core userland utilities (shell, coreutils) | **GPL-2.0** |
| iptables / ip6tables | Packet filtering used by dockerd for container networking | **GPL-2.0** |

**On the GPL-2.0 components (BusyBox, iptables).** BusyBox and iptables/ip6tables are GPL-2.0. In the LiteDock rootfs they are included as independent, separately compiled programs (the standard Alpine binary packages) that are merely **aggregated** alongside the Apache-2.0 engine binaries on the same filesystem image — they are not statically or dynamically linked into LiteDock's own code or into the Docker Engine. Distributing them as part of a Linux rootfs is the ordinary way they are shipped (BusyBox is the standard userland of virtually every Alpine image; dockerd shells out to the unmodified iptables binaries). LiteDock's own application code does not derive from or link against them, so redistributing the rootfs as a whole is fine. To satisfy GPL-2.0, the rootfs retains their license texts and the corresponding source is available from the upstream projects / Alpine package mirrors.

## Desktop application

| Component | Role | License |
| --- | --- | --- |
| Tauri (framework + bundler) | Desktop shell, IPC, NSIS bundling | MIT or Apache-2.0 (dual) |
| bollard | Rust Docker Engine API client | Apache-2.0 |
| Svelte | Frontend UI framework | MIT |

The app also depends, transitively, on many MIT/Apache-2.0/BSD-licensed Rust crates and npm packages pulled in by Tauri, bollard, Vite, and the Svelte toolchain. Their license texts are produced as part of the normal dependency tooling (e.g. `cargo`/`npm` metadata) and are not enumerated individually here.

## What LiteDock does NOT include

- **Docker Desktop** — not bundled, not required, not redistributed.
- Any component under **Docker's commercial / Docker Subscription Service Agreement** terms.
- Docker, Inc. trademarks are not claimed; "Docker" is referenced only nominatively to describe the Apache-2.0 engine that LiteDock runs.

## Compliance note

LiteDock redistributes the above components in object/binary form inside the installer and the `litedock-engine` rootfs. To comply with the relevant licenses, distributions of LiteDock retain the upstream `LICENSE`/`NOTICE` files for the Apache-2.0 components, retain the MIT/BSD copyright notices for the Alpine/musl userland, and retain BusyBox's GPL-2.0 license text with a pointer to the corresponding source. Because all components are either permissive or (in the case of BusyBox) GPL-2.0 software that is aggregated rather than linked, redistributing LiteDock as an installer plus rootfs is compatible with these licenses. This note is informational and is not legal advice; downstream redistributors should review the upstream license files shipped in the rootfs.
