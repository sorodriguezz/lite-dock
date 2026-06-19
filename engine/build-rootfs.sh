#!/usr/bin/env bash
# =============================================================================
#  LiteDock — build the engine rootfs tarball
# =============================================================================
#
#  WHAT THIS DOES
#  --------------
#  Builds engine/Dockerfile.engine and EXPORTS the resulting container's
#  filesystem to a flat tarball at:
#
#      src-tauri/resources/litedock-engine.tar
#
#  That tarball is what the Windows app feeds to `wsl --import`.
#
#  WHY `docker export` AND NOT `docker save`
#  -----------------------------------------
#  `wsl --import` wants a *flat root filesystem* tar: a single archive whose
#  top level is /bin, /etc, /usr, ... `docker save` produces an OCI image
#  archive (layers + manifest + config json) which WSL cannot consume.
#  `docker export` flattens all layers of a *container* into exactly the flat
#  rootfs we need. So the flow is: build image -> create (not start) a
#  container from it -> export that container -> discard the container.
#
#  WHERE TO RUN THIS
#  -----------------
#  Anywhere with a working Docker CLI + daemon and BuildKit:
#    * inside an existing WSL2 Ubuntu distro (easiest on a dev's Windows box),
#    * on native Linux,
#    * on macOS with Docker Desktop / colima,
#    * or any CI runner with Docker.
#  It does NOT need to run on the target machine; the tarball is portable.
#
#  This script is idempotent and safe to re-run: it always rebuilds from the
#  Dockerfile, overwrites the previous tarball, and cleans up its temp
#  container even if an earlier run was interrupted.
# =============================================================================

set -euo pipefail

# -----------------------------------------------------------------------------
# Resolve paths.
#
# We compute the engine/ directory from the script's own location, then the
# project root is its parent. This means the script works no matter what the
# caller's current working directory is (e.g. `bash engine/build-rootfs.sh`
# from the project root, or running it directly from inside engine/).
# -----------------------------------------------------------------------------
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd -P)"
PROJECT_ROOT="$(cd -- "${SCRIPT_DIR}/.." >/dev/null 2>&1 && pwd -P)"

# Canonical constants — MUST match the rest of LiteDock.
IMAGE_TAG="litedock-engine-rootfs"          # local-only build tag, never pushed
CONTAINER_NAME="litedock-engine-export"     # transient container we export
OUTPUT_REL="src-tauri/resources/litedock-engine.tar"
OUTPUT_TAR="${PROJECT_ROOT}/${OUTPUT_REL}"
DOCKERFILE="${SCRIPT_DIR}/Dockerfile.engine"
BUILD_CONTEXT="${SCRIPT_DIR}"               # engine/ — holds Dockerfile + files/

# -----------------------------------------------------------------------------
# Small helpers for readable progress output.
# -----------------------------------------------------------------------------
log()  { printf '\033[1;34m[litedock]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[litedock]\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m[litedock] ERROR:\033[0m %s\n' "$*" >&2; exit 1; }

# -----------------------------------------------------------------------------
# Cleanup trap: always remove the temp container, even on failure / Ctrl-C.
# `docker rm -f` is a no-op (well, an ignorable error) if it doesn't exist, so
# we guard with `|| true` to keep `set -e` happy.
# -----------------------------------------------------------------------------
cleanup() {
    docker rm -f "${CONTAINER_NAME}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

# -----------------------------------------------------------------------------
# Preflight checks.
# -----------------------------------------------------------------------------
command -v docker >/dev/null 2>&1 \
    || die "docker not found. No Docker? Build the engine WITHOUT it, inside WSL: 'bash engine/build-rootfs-wsl.sh'"

docker info >/dev/null 2>&1 \
    || die "Cannot talk to the Docker daemon. No Docker daemon available? Build the engine WITHOUT Docker, inside WSL: 'bash engine/build-rootfs-wsl.sh' (see docs/BUILD.md)."

[ -f "${DOCKERFILE}" ] \
    || die "Dockerfile not found at ${DOCKERFILE}"

log "Project root : ${PROJECT_ROOT}"
log "Dockerfile   : ${DOCKERFILE}"
log "Output tar   : ${OUTPUT_TAR}"

# -----------------------------------------------------------------------------
# Clean up any leftover temp container from a previous interrupted run BEFORE
# we start, so `docker create` below can reuse the fixed name.
# -----------------------------------------------------------------------------
cleanup

# Make sure the output directory exists (src-tauri/resources may not yet).
mkdir -p "$(dirname -- "${OUTPUT_TAR}")"

# -----------------------------------------------------------------------------
# 1) Build the image.
#
#    --platform linux/amd64 : the target is x64 Windows/WSL2. We force the
#                             platform so building on an arm64 host (Apple
#                             Silicon, arm CI) still produces an amd64 rootfs.
#                             Requires binfmt/qemu on non-amd64 hosts.
#    --pull                 : always re-resolve the pinned alpine base so a
#                             locally stale layer can't poison the build.
# -----------------------------------------------------------------------------
log "Building engine image (${IMAGE_TAG}) for linux/amd64 ..."
DOCKER_BUILDKIT=1 docker build \
    --platform linux/amd64 \
    --pull \
    -f "${DOCKERFILE}" \
    -t "${IMAGE_TAG}" \
    "${BUILD_CONTEXT}"

# -----------------------------------------------------------------------------
# 2) Create (do NOT start) a container from the image.
#
#    We never need the daemon to actually run here — we only want a container
#    whose filesystem we can flatten. `docker create` gives us exactly that
#    without executing anything. --platform keeps us consistent with the build.
# -----------------------------------------------------------------------------
log "Creating transient container (${CONTAINER_NAME}) ..."
docker create --platform linux/amd64 --name "${CONTAINER_NAME}" "${IMAGE_TAG}" >/dev/null

# -----------------------------------------------------------------------------
# 3) Export the container filesystem to the flat rootfs tarball.
#
#    We write to a temporary file first and then atomically move it into place,
#    so a partially-written tar can never be left behind if export fails midway.
# -----------------------------------------------------------------------------
log "Exporting rootfs to ${OUTPUT_REL} ..."
TMP_TAR="${OUTPUT_TAR}.partial"
docker export "${CONTAINER_NAME}" -o "${TMP_TAR}"
mv -f "${TMP_TAR}" "${OUTPUT_TAR}"

# -----------------------------------------------------------------------------
# 4) Remove the temp container (the EXIT trap also covers this; explicit here
#    so the success path leaves nothing behind immediately).
# -----------------------------------------------------------------------------
log "Removing transient container ..."
docker rm -f "${CONTAINER_NAME}" >/dev/null 2>&1 || true

# -----------------------------------------------------------------------------
# 5) Report the resulting size. Uncompressed rootfs target is < 150 MB; a
#    typical build lands around 250-350 MB because of the Docker/BuildKit/
#    containerd/runc binaries (these dominate; the Alpine base is ~8 MB).
# -----------------------------------------------------------------------------
[ -f "${OUTPUT_TAR}" ] || die "Export finished but ${OUTPUT_TAR} is missing."
SIZE="$(du -h "${OUTPUT_TAR}" | cut -f1)"

log "Done."
log "Rootfs tarball: ${OUTPUT_TAR}"
log "Size         : ${SIZE} (uncompressed flat rootfs)"
log ""
log "Next: the Windows app imports it with:"
log "  wsl --import litedock-engine <dataDir> ${OUTPUT_REL//\//\\} --version 2"
