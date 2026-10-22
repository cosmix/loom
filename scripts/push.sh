#!/usr/bin/env bash
# Run the pre-push checks before git opens any connection, then push.
# git push connects to the remote before it runs the hook, and the checks
# take long enough for the remote to close an idle connection, which makes
# git exit 141 silently with nothing pushed. Here the hook records HEAD as
# verified first, so the real push's hook returns at once on a fresh connection.
#
# Usage: scripts/push.sh [git push arguments]
#
# Same arguments as `git push`. Checks the commit HEAD of the current branch,
# then pushes. The checks run in a detached worktree at
# ${XDG_CACHE_HOME:-$HOME/.cache}/loom-push-check/<id> (<id> hashes this
# repository's git directory), so uncommitted and untracked files in this
# checkout never affect them. That worktree keeps its own loom/target: the
# first run builds from scratch, later runs build incrementally. A push of
# anything other than the current branch's HEAD still runs the checks in the
# hook.
set -euo pipefail

branch="$(git symbolic-ref -q --short HEAD || true)"
if [ -z "$branch" ]; then
  echo "push.sh: HEAD is detached; check out a branch first." >&2
  exit 1
fi

head_sha="$(git rev-parse HEAD)"
common_dir="$(git rev-parse --path-format=absolute --git-common-dir)"
cache_dir="${XDG_CACHE_HOME:-$HOME/.cache}/loom-push-check"
mkdir -p "$cache_dir"
cache_dir="$(cd "$cache_dir" && pwd -P)"
# cksum is POSIX, so the id is the same on Linux and macOS.
id="$(printf '%s' "$common_dir" | cksum | cut -d' ' -f1)"
wt="$cache_dir/$id"
lock="$wt.lock"

if ! mkdir "$lock" 2>/dev/null; then
  echo "push.sh: another push.sh is using $wt ($(cat "$lock/owner" 2>/dev/null || echo 'owner unknown'))." >&2
  echo "push.sh: wait for it, or remove $lock if that run died." >&2
  exit 1
fi
release_lock() { rm -rf "$lock"; }
trap release_lock EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
echo "pid $$ on $(hostname), started $(date '+%Y-%m-%d %H:%M:%S')" > "$lock/owner"

# A usable check worktree is registered with this repository and its .git
# link resolves. Anything else at that path is removed and recreated.
worktree_usable() {
  [ -e "$wt/.git" ] \
    && git worktree list --porcelain | grep -qx "worktree $wt" \
    && [ "$(git -C "$wt" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" = "$common_dir" ]
}

if worktree_usable; then
  git -C "$wt" checkout -q --detach --force "$head_sha"
  git -C "$wt" clean -fdq
else
  rm -rf "$wt"
  git worktree prune
  git worktree add -q --detach "$wt" "$head_sha"
fi

echo "push.sh: checking ${head_sha:0:12} in $wt"
zeros="0000000000000000000000000000000000000000"
if ! printf '%s %s %s %s\n' "refs/heads/$branch" "$head_sha" "refs/heads/$branch" "$zeros" \
  | (cd "$wt" && LOOM_PUSH_CHECK=1 "$wt/loom/.githooks/pre-push" origin ""); then
  echo "push.sh: checks failed; nothing pushed." >&2
  exit 1
fi

if ! grep -qx "$head_sha" "$common_dir/loom-pre-push-verified" 2>/dev/null; then
  echo "push.sh: checks passed but ${head_sha:0:12} was not recorded as verified; nothing pushed." >&2
  exit 1
fi

# The push replaces this process, so the EXIT trap would not run: release now.
release_lock
trap - EXIT
exec git push "$@"
