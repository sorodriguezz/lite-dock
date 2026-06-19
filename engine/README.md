# LiteDock — Engine (Linux) Packaging

This directory builds the **`litedock-engine`** WSL2 distro: a minimal Alpine
root filesystem that runs the real upstream **Docker Engine** (`dockerd`),
`containerd`, `runc`, **BuildKit** (`buildx`) and **Compose v2** inside a
dedicated, throwaway WSL2 distro on the user's Windows machine.

LiteDock is a lightweight alternative to Docker Desktop. Instead of bundling a
custom VM, it leans on the Microsoft-provided WSL2 kernel and ships only this
tiny purpose-built distro to host the engine.

---

## What this distro is — and why it's separate

- **It is its own WSL2 distro**, named exactly `litedock-engine`. It is
  **not** installed into, and does **not** touch, any of the user's own WSL
  distros (Ubuntu, Debian, etc.). Keeping it isolated means:
  - The Docker data-root (`/var/lib/docker`) lives in LiteDock's own VHDX, so
    uninstalling LiteDock (just unregister the distro) reclaims all space and
    leaves the user's distros untouched.
  - We fully control the contents (packages, init, iptables backend) — nothing
    the user did to their Ubuntu can break the engine.
- **No systemd, no service manager.** WSL2 distros are a rootfs on a shared
  kernel; there's no init system here. `dockerd` is started **on demand** by
  the Windows app and stopped by tearing the distro down. When you're not using
  Docker, the distro is idle at ~0 RAM/CPU.

### Lifecycle (how the Windows app drives it)

| Action | Command the app runs |
| --- | --- |
| Import (first run) | `wsl --import litedock-engine <dataDir> src-tauri\resources\litedock-engine.tar --version 2` |
| Start engine | `wsl -d litedock-engine -u root --exec /usr/local/bin/litedock-init.sh` |
| Stop engine | `wsl --terminate litedock-engine` |
| Remove | `wsl --unregister litedock-engine` |

`litedock-init.sh` runs `dockerd` in the **foreground** and `exec`s into it, so
WSL holds the process handle directly and `wsl --terminate` is a clean stop.

### Canonical constants (other parts of the app depend on these)

| Thing | Value |
| --- | --- |
| WSL distro name | `litedock-engine` |
| dockerd TCP host | `tcp://127.0.0.1:23750` |
| dockerd UNIX host | `unix:///var/run/docker.sock` |
| Docker data-root | `/var/lib/docker` |
| Init script path | `/usr/local/bin/litedock-init.sh` |
| Output rootfs tar | `src-tauri/resources/litedock-engine.tar` |

> **Why the hosts are passed as CLI flags, not in `daemon.json`:** dockerd
> refuses to start with *"unable to configure the Docker daemon ... hosts in
> daemon.json and CLI conflict"* if a `hosts` key in `daemon.json` overlaps
> with `-H/--host` flags. The init script owns the `--host` flags, so
> `daemon.json` deliberately omits `hosts`.

---

## Files in this directory

| File | Purpose |
| --- | --- |
| `Dockerfile.engine` | Assembles the Alpine rootfs (packages + config). Exported, never run as a normal container. |
| `build-rootfs.sh` | Builds the image and `docker export`s it to the rootfs tarball. |
| `files/litedock-init.sh` | Foreground entrypoint: sets up runtime, then `exec dockerd`. |
| `files/wsl.conf` | `/etc/wsl.conf` — automount, DNS, default user, **systemd off**. |
| `files/daemon.json` | `/etc/docker/daemon.json` — minimal dockerd config (no `hosts`). |
| `README.md` | This file. |

---

## Building the rootfs tarball

### Prerequisites

You need **a working Docker host with BuildKit** to build the tarball. You do
**not** need the target machine — the artifact is portable. Easiest options:

- **Inside an existing WSL2 Ubuntu** on the dev's Windows box
  (`sudo apt install docker.io`, or Docker's official repo), **or**
- **Native Linux** with Docker installed, **or**
- **macOS** with Docker Desktop / colima, **or**
- **Any CI runner** with Docker.

> Building on a non-amd64 host (e.g. Apple Silicon) works because the build
> forces `--platform linux/amd64`, but you'll need QEMU/binfmt registered
> (`docker run --privileged --rm tonistiigi/binfmt --install amd64`).

### Command

From the **project root**:

```bash
bash engine/build-rootfs.sh
```

This will:

1. `docker build --platform linux/amd64 -f engine/Dockerfile.engine ... engine/`
2. `docker create` a transient container from the image,
3. `docker export` its filesystem to `src-tauri/resources/litedock-engine.tar`,
4. remove the temp container, and
5. print the resulting tarball size.

The script is **idempotent** — re-run it any time; it rebuilds and overwrites
the tarball and cleans up after itself (even on Ctrl-C).

> The tarball is a **build artifact** and should be `.gitignore`d, not
> committed. CI (or the dev) regenerates it before packaging the installer.

### Importing it manually (for testing)

```powershell
wsl --import litedock-engine "$env:LOCALAPPDATA\LiteDock\distro" `
    src-tauri\resources\litedock-engine.tar --version 2
wsl -d litedock-engine -u root --exec /usr/local/bin/litedock-init.sh
# In another terminal:
docker -H tcp://127.0.0.1:23750 version
```

---

## Size

- **Target:** uncompressed flat rootfs ideally **< 150 MB**.
- **Reality:** expect roughly **~250–350 MB uncompressed**. The Alpine base is
  only ~8 MB; the weight is the engine stack itself — `dockerd`, `containerd`,
  `runc`, the `buildx` plugin, and the `compose` plugin are large Go binaries.
  A few hundred MB is on the high side; the lever to pull is **which packages
  ship**, not the base.
- **Keeping it lean (what we already do):**
  - Alpine + musl + busybox base (no glibc, no coreutils bloat).
  - `apk add --no-cache` so no package index is left in the rootfs.
  - No `bash` (scripts are POSIX `sh` / busybox `ash`).
  - No docs/man, no compilers, no extra tooling.
- **Further trimming if needed (future):** drop `buildx`/`compose` for a
  "core" build, strip binaries, or `xz`-compress the tar at rest and
  decompress on import. WSL imports an uncompressed tar, so compression only
  helps the installer download, not the on-disk distro.

---

## Storage driver note (overlay2 vs vfs)

The engine uses **`overlay2`** (fast, space-efficient). On a small number of
WSL2 setups overlay2 can fail if the backing filesystem lacks the required
overlay features. If `dockerd` errors at startup with a storage-driver/overlay
mount failure, switch to **`vfs`** (slower, more disk, but works everywhere):
in `files/litedock-init.sh` comment out the `overlay2` `exec dockerd` block and
uncomment the `vfs` one (and keep `daemon.json` in sync). This is documented
inline in the init script.

---

## Licensing / redistribution

This distro bundles upstream open-source software. All components below are
freely **redistributable** under permissive licenses, which is what lets
LiteDock ship the prebuilt rootfs tarball.

| Component | Upstream | License |
| --- | --- | --- |
| Docker Engine (`dockerd`) / Moby | github.com/moby/moby | Apache-2.0 |
| Docker CLI | github.com/docker/cli | Apache-2.0 |
| BuildKit / `buildx` | github.com/docker/buildx, moby/buildkit | Apache-2.0 |
| Compose v2 | github.com/docker/compose | Apache-2.0 |
| containerd | github.com/containerd/containerd | Apache-2.0 |
| runc | github.com/opencontainers/runc | Apache-2.0 |
| Alpine base + musl libc | alpinelinux.org / musl-libc.org | musl: MIT; Alpine pkgs: mostly MIT / BSD / GPL per package |
| iptables / ip6tables | netfilter.org | GPL-2.0 (used as unmodified binaries) |
| BusyBox (base userland) | busybox.net | GPL-2.0 |

Notes:

- The Docker/Moby/BuildKit/containerd/runc/Compose stack is **Apache-2.0** —
  redistribution requires preserving license/notice files, which Alpine's
  packages already place under `/usr/share/licenses` (and we ship them in the
  rootfs).
- GPL components (iptables, BusyBox) are redistributed **unmodified** as
  Alpine binary packages; their source is available from Alpine's mirrors. We
  do not statically link them into LiteDock's proprietary code.
- Docker® is a trademark of Docker, Inc. LiteDock is **not** affiliated with or
  endorsed by Docker, Inc.; we redistribute the open-source engine only.

A consolidated `THIRD-PARTY-NOTICES` file for the installer can be generated
from the rootfs (`/usr/share/licenses`) at packaging time.

---

## Future work: arm64 (Windows on ARM)

WSL2 also runs on **arm64 Windows** (Surface Pro X, Snapdragon laptops). To
support it we'll build an **arm64 variant** of this rootfs:

- Change/parametrize the build to `--platform linux/arm64` and produce a second
  tarball (e.g. `litedock-engine-arm64.tar`).
- The Windows app picks the tarball matching the host architecture at import
  time.
- All packages above exist for `aarch64` in Alpine, so no source changes are
  expected — only the build platform and the artifact name differ.
