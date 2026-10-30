#!/usr/bin/env bash
# Drive the planner's user journeys (J1-J8) in a headless browser against a real `loom plan` server.
#
# Usage: scripts/plan-journeys.sh [--strict] <loom-binary> <out-dir>
#   e.g. scripts/plan-journeys.sh loom/target/debug/loom "$LOOM_SCRATCH_DIR/journeys"
#
# Run from the worktree root after `cd web && bun run build` and `cargo build` (the binary embeds
# web/dist). Prints one line per journey: PASS, FAIL <why> or PENDING <what is missing>, with a
# screenshot per step under <out-dir>. --strict turns PENDING into a failure. Exits non-zero on a
# FAIL, a console or page error, or (strict) a PENDING.
#
# The server runs on a scratch clone of HEAD with the worktree's doc/ copied over it, with HOME in
# the scratch directory and a fake `claude` first on PATH, so nothing touches the operator's
# ~/.claude.json. The browser and this script keep the real HOME: playwright finds its browser
# cache through it. J8 (sessions end with the server) runs here, after the browser closes.
set -euo pipefail

strict=0
if [[ "${1:-}" == "--strict" ]]; then
  strict=1
  shift
fi
if [[ $# -ne 2 ]]; then
  cat >&2 <<'USAGE'
usage: scripts/plan-journeys.sh [--strict] <loom-binary> <out-dir>
USAGE
  exit 2
fi
binary=$(realpath "$1")
out_dir=$2

repo=$(git rev-parse --show-toplevel)
script_dir=$(cd "$(dirname "$0")" && pwd)
mjs="$script_dir/../web/scripts/plan-journeys.mjs"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/plan-journeys.XXXXXX")
started_at=$(date +%s)
socket_dir="${TMUX_TMPDIR:-/tmp}/tmux-$(id -u)"
probe="loom-journeys-probe-$$"
server_pid=""

# `loom plan` names its tmux sockets loom-plan-<id>. Kill only those whose server started after
# this script did: an older one may be the operator's own.
kill_new_sockets() {
  local socket name started
  for socket in "$socket_dir"/loom-plan-*; do
    [[ -S "$socket" ]] || continue
    name=$(basename "$socket")
    started=$(tmux -L "$name" list-sessions -F '#{start_time}' 2>/dev/null | head -n 1 || true)
    if [[ "$started" =~ ^[0-9]+$ && "$started" -ge "$started_at" ]]; then
      tmux -L "$name" kill-server 2>/dev/null || true
    fi
  done
}

cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill -TERM "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  kill_new_sockets
  tmux -L "$probe" kill-server 2>/dev/null || true
  rm -rf "$scratch"
}
trap cleanup EXIT

# Shallow clone, sparse to doc/. Detached HEAD: `loom plan` refuses a loom/<stage> branch.
clone="$scratch/clone"
git clone --quiet --depth 1 --no-checkout "file://$repo" "$clone"
git -C "$clone" sparse-checkout set doc
git -C "$clone" checkout --quiet --detach
rm -rf "$clone/doc"
cp -R "$repo/doc" "$clone/doc"

# The fake claude: only a launch carrying --session-id sleeps; everything else exits at once.
mkdir -p "$scratch/bin" "$scratch/home" "$scratch/loomhome"
cat >"$scratch/bin/claude" <<'FAKE'
#!/bin/sh
case "$*" in
  *--session-id*) exec sleep 98765 ;;
  *auth*status*) printf '{"loggedIn":true,"authMethod":"claude.ai"}\n' ;;
  *--version*) printf '9.9.9 (Claude Code)\n' ;;
  *) exit 0 ;;
esac
FAKE
chmod +x "$scratch/bin/claude"

# One probe decides whether a session start can succeed here: some sandboxes forbid the socket.
tmux_ok=0
if command -v tmux >/dev/null 2>&1 && tmux -L "$probe" new-session -d sleep 30 2>/dev/null; then
  tmux_ok=1
fi
tmux -L "$probe" kill-server 2>/dev/null || true

log="$scratch/server.log"
(
  cd "$clone"
  export HOME="$scratch/home" LOOM_HOME="$scratch/loomhome" PATH="$scratch/bin:$PATH"
  exec "$binary" plan --port 0 --no-open
) >"$log" 2>&1 &
server_pid=$!

# The token URL is printed within the first lines of the log.
url=""
for _ in $(seq 1 100); do
  url=$(grep -o -m1 -E 'http://[^ ]+/\?token=[^ ]+' "$log" || true)
  [[ -n "$url" ]] && break
  if ! kill -0 "$server_pid" 2>/dev/null; then
    echo "loom plan exited before printing a token URL:" >&2
    cat "$log" >&2
    server_pid=""
    exit 1
  fi
  sleep 0.1
done
if [[ -z "$url" ]]; then
  echo "no token URL after 10s; server log:" >&2
  cat "$log" >&2
  exit 1
fi

mjs_args=()
[[ $strict -eq 1 ]] && mjs_args+=(--strict)
[[ $tmux_ok -eq 0 ]] && mjs_args+=(--no-tmux)
status=0
node "$mjs" ${mjs_args[@]+"${mjs_args[@]}"} "$url" "$out_dir" "$clone" || status=$?

# J8: sessions end with the server. Sessions that J4 or J5 left running must go when the server
# is stopped by SIGTERM: no fake claude, and no tmux server of ours, ten seconds later.
fake_claudes() { pgrep -f '^sleep 98765$' || true; }

our_sockets() {
  local socket name started
  for socket in "$socket_dir"/loom-plan-*; do
    [[ -S "$socket" ]] || continue
    name=$(basename "$socket")
    started=$(tmux -L "$name" list-sessions -F '#{start_time}' 2>/dev/null | head -n 1 || true)
    if [[ "$started" =~ ^[0-9]+$ && "$started" -ge "$started_at" ]]; then
      echo "$name"
    fi
  done
  return 0
}

j8() {
  if [[ $tmux_ok -eq 0 ]]; then
    echo "PENDING J8 tmux cannot create a socket here"
    return 2
  fi
  if [[ -z "$(fake_claudes)" ]]; then
    echo "PENDING J8 no session was running to end (J4 and J5 started none)"
    return 2
  fi
  local sockets
  sockets=$(our_sockets)
  kill -TERM "$server_pid" 2>/dev/null || true
  for _ in $(seq 1 100); do
    kill -0 "$server_pid" 2>/dev/null || break
    sleep 0.1
  done
  if kill -0 "$server_pid" 2>/dev/null; then
    echo "FAIL J8 the server still runs 10 s after SIGTERM"
    return 1
  fi
  wait "$server_pid" 2>/dev/null || true
  server_pid=""
  for _ in $(seq 1 100); do
    [[ -z "$(fake_claudes)" && -z "$(our_sockets)" ]] && break
    sleep 0.1
  done
  local left
  left=$(fake_claudes | wc -l)
  if [[ $left -gt 0 ]]; then
    echo "FAIL J8 $left fake claude process(es) outlived the server by 10 s"
    return 1
  fi
  if [[ -n "$(our_sockets)" ]]; then
    echo "FAIL J8 tmux server(s) outlived the server by 10 s: $(our_sockets | tr '\n' ' ')"
    return 1
  fi
  echo "PASS J8 the server stopped on SIGTERM and took ${sockets:+its tmux sockets and }the session with it"
}

j8_status=0
j8 || j8_status=$?
if [[ $j8_status -eq 1 || ( $j8_status -eq 2 && $strict -eq 1 ) ]]; then
  status=1
fi
exit "$status"
