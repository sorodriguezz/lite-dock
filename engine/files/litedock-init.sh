#!/bin/sh
# =============================================================================
#  LiteDock — engine init / foreground entrypoint
# =============================================================================
#
#  HOW THIS IS LAUNCHED
#  --------------------
#  The Windows app starts the engine by spawning:
#
#      wsl -d litedock-engine -u root --exec /usr/local/bin/litedock-init.sh
#
#  The very last thing this script does is `exec dockerd ...`, replacing this
#  shell with the dockerd process. That matters: WSL keeps a handle on the
#  process it launched, so when the Windows side wants to STOP the engine it
#  simply runs `wsl --terminate litedock-engine`, which tears down the distro
#  and with it dockerd. Running dockerd in the FOREGROUND (no &, no daemon
#  fork) is what makes that lifecycle clean — there is no orphaned background
#  daemon to chase.
#
#  WHY A SCRIPT AND NOT JUST `dockerd`
#  -----------------------------------
#  WSL2 has no systemd/OpenRC by default, so all the one-time setup a normal
#  distro's init would do has to happen here first: create runtime dirs, pin
#  the iptables backend, enable IP forwarding, and make sure cgroup v2 is
#  available. Each step is guarded so re-launching the engine is harmless.
#
#  SHELL: this is POSIX sh (busybox ash on Alpine). No bashisms.
# =============================================================================

set -eu

# Ensure the standard sbin dirs are on PATH. A bare `wsl --exec` can start with
# a minimal PATH, and dockerd shells out to `iptables`, `modprobe`, etc. — if
# they aren't on PATH, bridge-network setup fails with "iptables not found".
export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"

# Canonical constants — these MUST match the rest of LiteDock.
DOCKER_HOST_TCP="tcp://127.0.0.1:23750"
DOCKER_HOST_UNIX="unix:///var/run/docker.sock"
DATA_ROOT="/var/lib/docker"
LOG_FILE="/var/log/litedock-dockerd.log"

log() { printf '[litedock-init] %s\n' "$*"; }

log "starting engine init ..."

# -----------------------------------------------------------------------------
# 1) Runtime directories.
#
# Nothing boots us, so we cannot assume these exist or survived a previous
# `wsl --terminate`. /var/run is frequently a fresh tmpfs each launch, so the
# docker.sock parent in particular must be (re)created every time.
# -----------------------------------------------------------------------------
log "ensuring runtime directories ..."
mkdir -p /var/run
mkdir -p "${DATA_ROOT}"
mkdir -p /var/log
# data-root should not be world-traversable (matches dockerd's own default).
chmod 0711 "${DATA_ROOT}" 2>/dev/null || true

# -----------------------------------------------------------------------------
# 2) iptables backend -> legacy.
#
# The Dockerfile already repointed these symlinks, but an `apk upgrade` inside
# the running distro (or a user poking around) could have reset them. dockerd's
# bridge NAT is most reliable on iptables-legacy under the WSL2 kernel, which
# may lack full nf_tables support. Re-assert the symlinks every launch; this is
# cheap and idempotent.
# -----------------------------------------------------------------------------
# Self-heal iptables. Some rootfs builds leave the iptables APK marked installed
# but WITHOUT the actual binary, so dockerd fails to create the bridge NAT chain
# ("iptables not found"). Repair the files; if there's still no bare `iptables`,
# symlink it to whatever nft/legacy variant exists.
if ! command -v iptables >/dev/null 2>&1; then
    log "iptables not usable — repairing via apk ..."
    apk fix iptables >/dev/null 2>&1 || apk add --no-cache iptables >/dev/null 2>&1 || true
fi
if ! command -v iptables >/dev/null 2>&1; then
    for v in /usr/sbin/iptables-nft /sbin/iptables-nft /usr/sbin/iptables-legacy /sbin/iptables-legacy; do
        if [ -e "$v" ]; then
            ln -sf "$v" /usr/sbin/iptables
            ln -sf "$v" /sbin/iptables
            log "linked iptables -> $v"
            break
        fi
    done
fi
if command -v iptables >/dev/null 2>&1; then
    log "iptables ok: $(command -v iptables)"
else
    log "warning: iptables still unavailable — bridge networking may fail."
fi

if [ -x /sbin/iptables-legacy ]; then
    log "selecting iptables-legacy backend ..."
    ln -sf /sbin/iptables-legacy  /sbin/iptables  2>/dev/null || true
    ln -sf /sbin/ip6tables-legacy /sbin/ip6tables 2>/dev/null || true
else
    log "iptables-legacy not present; leaving iptables backend as-is."
fi

# -----------------------------------------------------------------------------
# 3) IPv4 forwarding.
#
# dockerd needs net.ipv4.ip_forward=1 to route traffic between the docker0
# bridge and the outside world. dockerd will try to set this itself, but doing
# it up front avoids a startup warning and works even if dockerd's attempt is
# blocked. Guard the write because /proc/sys may be read-only in odd setups.
# -----------------------------------------------------------------------------
log "enabling net.ipv4.ip_forward ..."
if [ -w /proc/sys/net/ipv4/ip_forward ]; then
    echo 1 > /proc/sys/net/ipv4/ip_forward 2>/dev/null || true
else
    # sysctl may still succeed where the direct write path check failed.
    sysctl -w net.ipv4.ip_forward=1 >/dev/null 2>&1 || \
        log "warning: could not enable ip_forward (dockerd will retry)."
fi

# -----------------------------------------------------------------------------
# 4) cgroup v2.
#
# Modern dockerd + containerd want a unified cgroup v2 hierarchy mounted at
# /sys/fs/cgroup. The WSL2 kernel almost always provides this already, so the
# common case is "detect and move on". We only attempt a mount as a fallback,
# and we NEVER hard-fail here: if cgroups are genuinely unavailable, we let
# dockerd surface the real, specific error rather than masking it.
# -----------------------------------------------------------------------------
if [ -f /sys/fs/cgroup/cgroup.controllers ]; then
    log "cgroup v2 already mounted."
elif [ -d /sys/fs/cgroup ] && ! mountpoint -q /sys/fs/cgroup 2>/dev/null; then
    log "cgroup v2 not mounted; attempting to mount ..."
    mount -t cgroup2 none /sys/fs/cgroup 2>/dev/null \
        && log "mounted cgroup v2." \
        || log "warning: cgroup2 mount failed (kernel may use a different layout)."
else
    log "cgroup hierarchy present (assuming usable)."
fi

# -----------------------------------------------------------------------------
# 5) Launch dockerd in the FOREGROUND.
#
# The listening sockets are passed here as --host flags (NOT in daemon.json) to
# avoid the "hosts in daemon.json and CLI conflict" startup error. We bind:
#     * tcp://127.0.0.1:23750  -> how the Windows app / Tauri backend talks to
#                                 the engine across the WSL boundary.
#     * unix:///var/run/docker.sock -> the classic socket, for tools running
#                                 inside the distro and for compatibility.
#
# --data-root and --storage-driver are explicit so behaviour is deterministic
# regardless of what dockerd would otherwise auto-detect.
#
# `exec` replaces this shell with dockerd so WSL's process handle == dockerd,
# which is what makes `wsl --terminate` a clean stop.
#
# LOGGING TRADE-OFF: we want dockerd in the foreground AND a copy of its log on
# disk. The naive `exec dockerd ... | tee LOG` makes *tee* the head of the
# pipeline and dockerd a backgrounded child, so WSL's handle would point at the
# wrong process and termination would get messy. The correct trick is to NOT
# pipe dockerd at all: instead we create a FIFO, start a `tee` child reading
# from it, and exec-redirect THIS shell's fd 1/2 to the FIFO *before* exec'ing
# dockerd. dockerd then inherits those fds, so its output fans out to both the
# WSL console and LOG_FILE, while dockerd itself remains the single foreground
# process WSL tracks. If tee/mkfifo aren't available we simply skip the on-disk
# copy (console output still reaches the Windows app) and exec dockerd directly.
# -----------------------------------------------------------------------------
log "starting dockerd (foreground) ..."
log "  hosts      : ${DOCKER_HOST_TCP} , ${DOCKER_HOST_UNIX}"
log "  data-root  : ${DATA_ROOT}"
log "  storage    : overlay2"

# Make this script's own progress visible in the on-disk log too (best effort).
# We append our init banner so the log file has context before dockerd output.
{
    printf '==== litedock-init @ %s ====\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null || echo unknown)"
} > "${LOG_FILE}" 2>/dev/null || true

# Redirect this shell's stdout+stderr so that everything dockerd writes (it
# inherits our fds across exec) lands in BOTH the WSL console and the log file.
# `tee` here runs as a child reading from a FIFO we attach as our fd 1/2; dockerd
# remains the foreground exec'd process and WSL's handle still points at it,
# preserving clean `wsl --terminate` behaviour.
if command -v tee >/dev/null 2>&1 && command -v mkfifo >/dev/null 2>&1; then
    FIFO="$(mktemp -u 2>/dev/null || echo /tmp/litedock-dockerd.fifo)"
    if mkfifo "${FIFO}" 2>/dev/null; then
        # Start the logger child: it copies anything on the FIFO to console+file.
        tee -a "${LOG_FILE}" < "${FIFO}" &
        # Point our fd 1 and 2 at the FIFO; dockerd inherits these on exec.
        exec > "${FIFO}" 2>&1
        # FIFO can be unlinked now; the open fds keep it alive until close.
        rm -f "${FIFO}" 2>/dev/null || true
    fi
fi

# -----------------------------------------------------------------------------
# Hand off to dockerd. After this line, dockerd IS this process.
#
# FALLBACK (overlay2 -> vfs): on a few WSL2 setups overlay2 fails because the
# underlying filesystem doesn't support the required overlay features. If you
# hit "driver not supported" / mount errors at startup, switch to vfs (slower,
# uses more disk, but works everywhere). To do that, comment out the overlay2
# exec below and uncomment the vfs one. Keep daemon.json's storage-driver in
# sync, or remove it there and let this flag win.
# -----------------------------------------------------------------------------

# --- overlay2 (default, fast) ---
exec dockerd \
    --host="${DOCKER_HOST_TCP}" \
    --host="${DOCKER_HOST_UNIX}" \
    --data-root="${DATA_ROOT}" \
    --storage-driver=overlay2

# --- vfs fallback (uncomment if overlay2 is unsupported on the host) ---
# exec dockerd \
#     --host="${DOCKER_HOST_TCP}" \
#     --host="${DOCKER_HOST_UNIX}" \
#     --data-root="${DATA_ROOT}" \
#     --storage-driver=vfs
