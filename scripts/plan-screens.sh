#!/usr/bin/env bash
# Screenshot planner routes in a headless browser against a real `loom plan` server.
#
# Usage: scripts/plan-screens.sh [--worktree-doc] <loom-binary> <out-dir> <route>...
#   e.g. scripts/plan-screens.sh loom/target/debug/loom "$LOOM_SCRATCH_DIR/screens" / /f/feat-planner
#
# Run from the worktree root after `cd web && bun run build` and `cargo build` (the binary embeds
# web/dist). Serves a shallow scratch clone of HEAD's doc/ so the operator commits nothing;
# --worktree-doc copies the worktree's current doc/ over it so uncommitted docs show. Shoots each
# route at 1440x900 and 390x844 in Ledger and Aubergine; exits non-zero on a console error or a
# route that does not render. Loopback is per Bash call, so the server and browser run here.
set -euo pipefail

worktree_doc=0
if [[ "${1:-}" == "--worktree-doc" ]]; then
  worktree_doc=1
  shift
fi
if [[ $# -lt 3 ]]; then
  sed -n '3,4p' "$0" >&2
  exit 2
fi
binary=$(realpath "$1")
out_dir=$2
shift 2

repo=$(pwd)
script_dir=$(cd "$(dirname "$0")" && pwd)
mjs="$script_dir/../web/scripts/plan-screens.mjs"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/plan-screens.XXXXXX")
server_pid=""

cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf "$scratch"
}
trap cleanup EXIT

# Shallow clone, sparse to doc/. Detached HEAD: `loom plan` refuses a loom/<stage> branch.
clone="$scratch/clone"
git clone --quiet --depth 1 --no-checkout "file://$repo" "$clone"
git -C "$clone" sparse-checkout set doc
git -C "$clone" checkout --quiet --detach
if [[ $worktree_doc -eq 1 ]]; then
  rm -rf "$clone/doc"
  cp -R "$repo/doc" "$clone/doc"
fi

mkdir -p "$scratch/loomhome"
log="$scratch/server.log"
(cd "$clone" && LOOM_HOME="$scratch/loomhome" exec "$binary" plan --port 0 --no-open) >"$log" 2>&1 &
server_pid=$!

# The token URL is printed within the first lines of the log.
url=""
for _ in $(seq 1 100); do
  url=$(grep -o -m1 -E 'http://[^ ]+/\?token=[^ ]+' "$log" || true)
  [[ -n "$url" ]] && break
  if ! kill -0 "$server_pid" 2>/dev/null; then
    echo "loom plan exited before printing a token URL:" >&2
    cat "$log" >&2
    exit 1
  fi
  sleep 0.1
done
if [[ -z "$url" ]]; then
  echo "no token URL after 10s; server log:" >&2
  cat "$log" >&2
  exit 1
fi

node "$mjs" "$url" "$out_dir" "$@"
