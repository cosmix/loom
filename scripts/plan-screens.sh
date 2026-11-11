#!/usr/bin/env bash
# Screenshot planner routes in a headless browser against a real `loom plan` server.
#
# Usage: scripts/plan-screens.sh [--worktree-doc] [--driver=<path>] [--themes=<id>,<id>] [--browser=chromium|firefox] <loom-binary> <out-dir> <route>...
#   e.g. scripts/plan-screens.sh loom/target/debug/loom "$LOOM_SCRATCH_DIR/screens" / /f/feat-planner
#
# Run from the worktree root after `cd web && bun run build` and `cargo build` (the binary embeds
# web/dist). Serves a shallow scratch clone of HEAD's doc/ so the operator commits nothing;
# --worktree-doc copies the worktree's current doc/ over it so uncommitted docs show. Shoots each
# route at 1440x900 and 390x844 in Ledger and Graphite, or the --themes ids (ledger, harbor,
# aubergine, graphite, midnight, obsidian, indigo), in Chromium unless --browser=firefox (the user's browser: use it for every visual check;
# the host's Playwright Firefox build must already be installed, nothing is downloaded). The leading
# flags may come in any order; exits non-zero on a console error or a route that does not render. Loopback
# is per Bash call, so the server and browser run here.
#
# --driver=<path> runs another browser script in place of web/scripts/plan-screens.mjs, with the
# same flags and arguments; a relative path is read from where this script is called.
# web/scripts/plan-chat-screens.mjs is one: it plays a chat session waiting on background tasks
# over the page's socket and shoots the chat panel, so give it a route with the panel (/files):
#   scripts/plan-screens.sh --driver=web/scripts/plan-chat-screens.mjs loom/target/debug/loom "$LOOM_SCRATCH_DIR/chat-screens" /files
set -euo pipefail

worktree_doc=0
driver=""
# Flags the browser script reads (--themes=, --browser=), passed through in the order given.
shoot_flags=()
while [[ "${1:-}" == --* ]]; do
  case "$1" in
    --worktree-doc) worktree_doc=1 ;;
    --driver=*) driver="${1#--driver=}" ;;
    --themes=* | --browser=*) shoot_flags+=("$1") ;;
    *)
      echo "unknown flag: $1" >&2
      exit 2
      ;;
  esac
  shift
done
if [[ $# -lt 3 ]]; then
  sed -n '3,4p' "$0" >&2
  exit 2
fi
binary=$(realpath "$1")
out_dir=$2
shift 2

repo=$(git rev-parse --show-toplevel)
script_dir=$(cd "$(dirname "$0")" && pwd)
mjs="${driver:-$script_dir/../web/scripts/plan-screens.mjs}"
if [[ ! -f "$mjs" ]]; then
  echo "no such driver: $mjs" >&2
  exit 2
fi
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

node "$mjs" ${shoot_flags[@]+"${shoot_flags[@]}"} "$url" "$out_dir" "$@"
