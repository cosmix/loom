#!/bin/bash
set -euo pipefail
# Loom container entrypoint.
#
# Per finding #15: the container starts as root just long enough to install
# the firewall, removes its own privilege escalation path, then drops to
# the unprivileged `loom` user via gosu. The agent process inside the
# container never has root or CAP_NET_ADMIN.
#
# === ROOT-ENTRY REQUIREMENT (M2, Codex blocker) =============================
# CAP_NET_ADMIN (needed by iptables/ipset) does not propagate from a
# non-root uid without ambient capability configuration, which loom does
# NOT set on the runtime --cap-add line. If a container runtime injects
# --user=<uid>:<gid> on `run`, this script starts non-root and the
# firewall install silently fails — leaving the agent with unrestricted
# outbound network access.
#
# Bail loudly when this happens so the operator can fix the runtime
# config. Stage 3 (harden-container-mod) removes the Docker --user
# injection so we always enter as uid=0.
if [ "$(id -u)" -ne 0 ]; then
    echo "ERROR: loom-entrypoint.sh must run as root (uid=0) to install" >&2
    echo "       the iptables firewall. Current uid=$(id -u)." >&2
    echo "       Remove --user from the container runtime invocation;" >&2
    echo "       entrypoint.sh drops to the 'loom' user via gosu after" >&2
    echo "       firewall install." >&2
    exit 78  # EX_CONFIG
fi

# Install the firewall (root-only — iptables and ipset both require
# CAP_NET_ADMIN, which is only available to uid=0 in our config).
/usr/local/bin/loom-firewall.sh

# Verify the firewall rules actually loaded before we drop privileges.
# If install silently no-op'd we'd otherwise gosu into an agent with full
# outbound network access. Match the marker the firewall script emits
# (`-m set --match-set loom-allowed dst -j ACCEPT`).
if ! iptables -S OUTPUT 2>/dev/null | grep -q "match-set loom-allowed"; then
    echo "ERROR: firewall install did not produce expected OUTPUT rules." >&2
    echo "       loom-allowed ipset rule missing from iptables OUTPUT chain." >&2
    echo "       Refusing to drop privileges — agent would have unrestricted" >&2
    echo "       network access." >&2
    exit 1
fi

# Remove the unprivileged loom user's path back to running the firewall
# script. After this rm, even if the agent escapes the workload sandbox it
# cannot re-run loom-firewall.sh (no sudoers entry, no setuid bit). M8.
if [ -e /etc/sudoers.d/loom-firewall ]; then
    rm /etc/sudoers.d/loom-firewall
fi

# === STAGE 4: Container-private git clone =====================================
# Stage 4 (isolated-git-architecture) eliminates the `.git` bind mount —
# the deepest container-escape path (Codex blocker B1). Instead of giving
# the container direct access to the host's git history, the orchestrator
# provisions a per-stage bare mirror at /var/loom/mirror (ro) and we
# clone it into /repo here, BEFORE exec-ing the unprivileged agent. The
# clone lives in the container's writable image layer (overlay2), so the
# agent has full freedom inside /repo without ever touching the host.
#
# Triggered by LOOM_BRANCH (and optionally LOOM_BASE_OID / LOOM_GIT_CLONE_DEPTH).
# When LOOM_BRANCH is unset we leave /repo untouched — that branch is the
# legacy/back-compat path used while in-container completion is still
# being migrated to host-authoritative RPC completion.
if [ -n "${LOOM_BRANCH:-}" ] && [ -d /var/loom/mirror ]; then
    LOOM_DEPTH="${LOOM_GIT_CLONE_DEPTH:-50}"
    # /repo is the container's clone destination. If anything already
    # lives there (e.g., from a stale prior run on this image), wipe
    # before cloning. `git clone` refuses a non-empty destination.
    if [ -d /repo ] && [ -n "$(ls -A /repo 2>/dev/null || true)" ]; then
        rm -rf /repo/* /repo/.[!.]* /repo/..?* 2>/dev/null || true
    fi
    mkdir -p /repo
    # git clone produces an `objects/info/alternates` only when the
    # source repo points at one — init_bare_mirror has already
    # verified the source has no alternates, so the clone here is
    # always self-contained.
    if ! git clone --depth="$LOOM_DEPTH" --branch="$LOOM_BRANCH" \
        /var/loom/mirror /repo 2>&1; then
        echo "ERROR: container-private clone from /var/loom/mirror failed." >&2
        echo "       Branch: $LOOM_BRANCH" >&2
        exit 1
    fi
    chown -R loom:loom /repo
fi

# Drop privileges and exec the agent workload.
exec gosu loom "$@"
