#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
#  Build the litedock-engine rootfs WITHOUT Docker.
#
#  Use this when you do NOT have a Docker daemon (the common case for LiteDock
#  users). It assembles the Alpine rootfs directly with `apk`, so the only
#  requirement is a real Linux environment — i.e. run it INSIDE WSL, with sudo.
#
#  It must NOT be run from Git Bash / MSYS (no chroot/mount there). Open your
#  WSL distro and run it from the repo checkout under /mnt/<drive>/...
#
#  Output:  src-tauri/resources/litedock-engine.tar   (flat rootfs for wsl --import)
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

ALPINE_VER="${ALPINE_VER:-3.20}"
ALPINE_PATCH="${ALPINE_PATCH:-3.20.3}"   # bump if the CDN 404s on the minirootfs
ARCH="${ARCH:-x86_64}"
MIRROR="${MIRROR:-https://dl-cdn.alpinelinux.org/alpine}"

# Packages that make up the engine (same set as engine/Dockerfile.engine).
# Note: the `iptables` package already provides the ip6tables binaries on
# Alpine (there is no separate `ip6tables` package).
PKGS="alpine-base docker docker-cli docker-cli-compose docker-cli-buildx containerd runc iptables ca-certificates e2fsprogs xz shadow tzdata"

log() { printf '\033[36m[litedock]\033[0m %s\n' "$*"; }
die() { printf '\033[31m[litedock] ERROR:\033[0m %s\n' "$*" >&2; exit 1; }

# ── sanity checks ────────────────────────────────────────────────────────────
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    die "Esto es Git Bash/MSYS, no un Linux real. Ábrelo dentro de WSL (p. ej. 'wsl' o 'wsl -d Ubuntu') y vuelve a correrlo." ;;
esac
command -v curl >/dev/null 2>&1 || die "Falta 'curl'. Instálalo (en Ubuntu: sudo apt-get install -y curl)."
command -v chroot >/dev/null 2>&1 || die "Falta 'chroot'. ¿Estás dentro de WSL?"

# chroot + bind mounts need root → re-exec with sudo if needed.
if [ "$(id -u)" -ne 0 ]; then
  log "Se requieren privilegios de root para chroot; reintentando con sudo…"
  exec sudo -E bash "$0" "$@"
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT="$ROOT_DIR/src-tauri/resources/litedock-engine.tar"

WORK="$(mktemp -d)"
ROOTFS="$WORK/rootfs"
mkdir -p "$ROOTFS"

cleanup() {
  for m in dev/pts dev proc sys; do
    mountpoint -q "$ROOTFS/$m" 2>/dev/null && umount -lf "$ROOTFS/$m" 2>/dev/null || true
  done
  rm -rf "$WORK"
}
trap cleanup EXIT

# ── 1. download + extract the Alpine minirootfs ──────────────────────────────
MINI="alpine-minirootfs-${ALPINE_PATCH}-${ARCH}.tar.gz"
URL="$MIRROR/v$ALPINE_VER/releases/$ARCH/$MINI"
log "Descargando $MINI…"
curl -fsSL "$URL" -o "$WORK/$MINI" || die "No se pudo descargar $URL (¿versión $ALPINE_PATCH correcta? prueba ALPINE_PATCH=3.20.x)."
log "Extrayendo rootfs base…"
tar -xzf "$WORK/$MINI" -C "$ROOTFS"

# ── 2. DNS + repositories ────────────────────────────────────────────────────
cp -f /etc/resolv.conf "$ROOTFS/etc/resolv.conf" 2>/dev/null || \
  printf 'nameserver 1.1.1.1\nnameserver 8.8.8.8\n' > "$ROOTFS/etc/resolv.conf"
printf '%s\n%s\n' "$MIRROR/v$ALPINE_VER/main" "$MIRROR/v$ALPINE_VER/community" \
  > "$ROOTFS/etc/apk/repositories"

# ── 3. bind mounts so apk's post-install scripts work ────────────────────────
mount --bind /proc "$ROOTFS/proc"
mount --bind /sys  "$ROOTFS/sys"
mount --bind /dev  "$ROOTFS/dev"

# ── 4. install the engine packages inside the chroot ─────────────────────────
log "Instalando el motor (dockerd, buildkit, compose…) con apk…"
chroot "$ROOTFS" /sbin/apk update
chroot "$ROOTFS" /sbin/apk add --no-cache $PKGS

# Verify the critical binaries actually landed — a silent apk miss here is what
# caused "iptables not found" at dockerd startup.
for b in dockerd docker containerd runc iptables; do
    found=""
    for d in sbin usr/sbin usr/bin bin usr/local/sbin usr/local/bin; do
        if [ -e "$ROOTFS/$d/$b" ]; then found=1; break; fi
    done
    [ -n "$found" ] || die "'$b' did not install into the rootfs — check apk repos/network"
done
log "verified present: dockerd, docker, containerd, runc, iptables"

# ── 5. iptables → legacy backend (WSL2 kernel friendliness) ──────────────────
chroot "$ROOTFS" /bin/sh -c \
  'ln -sf /sbin/iptables-legacy /sbin/iptables 2>/dev/null || true; \
   ln -sf /sbin/ip6tables-legacy /sbin/ip6tables 2>/dev/null || true'

# ── 6. drop in our config + init ─────────────────────────────────────────────
install -Dm755 "$SCRIPT_DIR/files/litedock-init.sh" "$ROOTFS/usr/local/bin/litedock-init.sh"
install -Dm644 "$SCRIPT_DIR/files/wsl.conf"         "$ROOTFS/etc/wsl.conf"
install -Dm644 "$SCRIPT_DIR/files/daemon.json"      "$ROOTFS/etc/docker/daemon.json"
mkdir -p "$ROOTFS/var/lib/docker" "$ROOTFS/var/log"

# Strip any Windows CRLF line endings — they break the shebang and config
# parsing inside Linux (a very common cause of "dockerd won't start").
sed -i 's/\r$//' \
  "$ROOTFS/usr/local/bin/litedock-init.sh" \
  "$ROOTFS/etc/wsl.conf" \
  "$ROOTFS/etc/docker/daemon.json"
chmod +x "$ROOTFS/usr/local/bin/litedock-init.sh"

# ── 7. unmount before tarring (don't capture /proc /sys /dev) ────────────────
for m in dev proc sys; do umount -lf "$ROOTFS/$m" 2>/dev/null || true; done

# ── 8. produce the flat rootfs tar for `wsl --import` ────────────────────────
mkdir -p "$(dirname "$OUT")"
log "Empaquetando $OUT…"
tar --numeric-owner -C "$ROOTFS" -cf "$OUT" .

log "✅ Listo: $OUT ($(du -h "$OUT" | cut -f1))"
log "Ahora, en Windows: npm install && npm run tauri build"
