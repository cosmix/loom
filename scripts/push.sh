#!/usr/bin/env bash
# Run the pre-push checks before git opens any connection, then push.
# git push connects to the remote before it runs the hook, and the checks
# take long enough for the remote to close an idle connection, which makes
# git exit 141 silently with nothing pushed. Here the hook records HEAD as
# verified first, so the real push's hook returns at once on a fresh connection.
#
# Usage: scripts/push.sh [git push arguments]
#
# Same arguments as `git push`. Verifies HEAD of the current branch, then
# pushes. A push of anything other than the current branch's HEAD still runs
# the checks in the hook.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"

if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
  echo "push.sh: tracked changes present; commit or stash them (the checks verify the working tree)." >&2
  exit 1
fi

branch="$(git symbolic-ref -q --short HEAD || true)"
if [ -z "$branch" ]; then
  echo "push.sh: HEAD is detached; check out a branch first." >&2
  exit 1
fi

head_sha="$(git rev-parse HEAD)"
zeros="0000000000000000000000000000000000000000"
printf '%s %s %s %s\n' "refs/heads/$branch" "$head_sha" "refs/heads/$branch" "$zeros" \
  | "$root/loom/.githooks/pre-push" origin ""

exec git push "$@"
