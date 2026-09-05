#!/usr/bin/env bash
# Repeat-under-load runner for timing-sensitive tests. The quota:: and
# process:: modules spawn subprocesses and assert on elapsed time, and the
# verdict_apply_tests:: and stalled_judge_tests:: modules kill a real judge
# process and assert on its death, so a race that fires once in ~15 runs
# still passes a single unloaded CI run most of the time. Running the
# filters --runs times under --load background CPU spinners reproduces the
# contention needed to flush the race out.
#
# A GitHub-hosted runner has 4 vCPUs; a workstation with spare cores can run
# the same filters without ever hitting the contention that fails there. When
# taskset is available (Linux) we pin the spinners and every test invocation
# to --cpus CPUs (default 4) so a local run reproduces CI's contention instead
# of a workstation's. macOS has no taskset, so there we run unpinned as
# before.
#
# Usage: scripts/flake-check.sh [--runs N] [--load N] [--cpus N] [filter ...]
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: flake-check.sh [--runs N] [--load N] [--cpus N] [filter ...]

  --runs N   repetitions per filter (default: 20)
  --load N   number of background CPU spinners (default: 2x --cpus when
             pinning is active, else nproc, else 4)
  --cpus N   number of CPUs to pin to via taskset, clamped to nproc
             (default: 4; ignored when taskset is unavailable, e.g. macOS)
  filter...  libtest name filters (default: "quota::" "process::"
             "verdict_apply_tests::" "stalled_judge_tests::")
  --help     show this help and exit
EOF
}

runs=20
cpus=4
load=
load_explicit=0
filters=()

while [ $# -gt 0 ]; do
  case "$1" in
    --runs)
      runs=$2
      shift 2
      ;;
    --load)
      load=$2
      load_explicit=1
      shift 2
      ;;
    --cpus)
      cpus=$2
      shift 2
      ;;
    --help)
      usage
      exit 0
      ;;
    -*)
      usage >&2
      exit 2
      ;;
    *)
      filters+=("$1")
      shift
      ;;
  esac
done

[ ${#filters[@]} -eq 0 ] && filters=("quota::" "process::" "verdict_apply_tests::" "stalled_judge_tests::")

pin=()
if command -v taskset >/dev/null 2>&1; then
  if nproc_count=$(nproc 2>/dev/null) && [ "$cpus" -gt "$nproc_count" ]; then
    cpus=$nproc_count
  fi
  cpu_list="0-$((cpus - 1))"
  pin=(taskset -c "$cpu_list")
  if [ "$load_explicit" -eq 0 ]; then
    load=$((2 * cpus))
  fi
  echo "pinning to CPUs $cpu_list (taskset); load=$load"
else
  if [ "$load_explicit" -eq 0 ]; then
    if load=$(nproc 2>/dev/null); then
      :
    else
      load=4
    fi
  fi
  echo "taskset unavailable; running unpinned; load=$load"
fi

repo_root=$(cd "$(dirname "$0")/.." && pwd)
cd "$repo_root/loom"

echo "compiling test binaries..."
cargo test --lib --no-run

spinner_pids=()
cleanup() {
  if [ ${#spinner_pids[@]} -gt 0 ]; then
    kill "${spinner_pids[@]}" 2>/dev/null || true
  fi
}
trap cleanup EXIT

if [ "$load" -gt 0 ]; then
  echo "starting $load CPU spinner(s)..."
  for _ in $(seq 1 "$load"); do
    ${pin[@]+"${pin[@]}"} sh -c 'while :; do :; done' &
    spinner_pids+=("$!")
  done
fi

for filter in "${filters[@]}"; do
  echo "== $filter: $runs run(s) under load=$load =="
  for i in $(seq 1 "$runs"); do
    out=$(mktemp "${TMPDIR:-/tmp}/flake-check.XXXXXX")
    if ! ${pin[@]+"${pin[@]}"} cargo test --lib -- "$filter" >"$out" 2>&1; then
      cleanup
      echo
      echo "FAILED: filter '$filter', run $i/$runs"
      cat "$out"
      rm -f "$out"
      exit 1
    fi
    rm -f "$out"
    printf '.'
  done
  echo
done

echo "flake-check: ok (filters: ${filters[*]}; $runs run(s) each; load=$load)"
