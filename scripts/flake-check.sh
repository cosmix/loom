#!/usr/bin/env bash
# Repeat-under-load runner for timing-sensitive tests. The quota:: and
# process:: modules spawn subprocesses and assert on elapsed time, and the
# verdict_apply_tests:: and stalled_judge_tests:: modules kill a real judge
# process and assert on its death, so a race that fires once in ~15 runs
# still passes a single unloaded CI run most of the time. Running the
# filters --runs times under --load background CPU spinners reproduces the
# contention needed to flush the race out.
#
# Usage: scripts/flake-check.sh [--runs N] [--load N] [filter ...]
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: flake-check.sh [--runs N] [--load N] [filter ...]

  --runs N   repetitions per filter (default: 20)
  --load N   number of background CPU spinners (default: nproc, else 4)
  filter...  libtest name filters (default: "quota::" "process::"
             "verdict_apply_tests::" "stalled_judge_tests::")
  --help     show this help and exit
EOF
}

runs=20
if load=$(nproc 2>/dev/null); then
  :
else
  load=4
fi
filters=()

while [ $# -gt 0 ]; do
  case "$1" in
    --runs)
      runs=$2
      shift 2
      ;;
    --load)
      load=$2
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
    (while :; do :; done) &
    spinner_pids+=("$!")
  done
fi

for filter in "${filters[@]}"; do
  echo "== $filter: $runs run(s) under load=$load =="
  for i in $(seq 1 "$runs"); do
    out=$(mktemp "${TMPDIR:-/tmp}/flake-check.XXXXXX")
    if ! cargo test --lib -- "$filter" >"$out" 2>&1; then
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
