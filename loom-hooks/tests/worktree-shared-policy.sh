#!/usr/bin/env bash
# Exercise installed guards against their shared policy, including data drift.
set -euo pipefail

SOURCE="$(cd "$(dirname "$0")/.." && pwd)"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-policy-test.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
HOOKS="$TMP/hooks"
WORKTREE="$TMP/repo/.worktrees/policy-test"
SHARED="$TMP/repo/.loom/work"
mkdir -p "$HOOKS" "$WORKTREE/.loom" "$WORKTREE/src" "$SHARED/handoffs"
cp "$SOURCE/"*.sh "$SOURCE/"*.py "$HOOKS/"
cp "$SOURCE/worktree-policy.json" "$HOOKS/"
ln -s "$SHARED" "$WORKTREE/.loom/work"

expect_status() {
    local expected="$1" hook="$2" payload="$3" output status=0
    output=$(cd "$WORKTREE" && printf '%s' "$payload" | bash "$HOOKS/$hook" 2>&1) || status=$?
    if [[ "$status" -ne "$expected" ]]; then
        printf 'FAIL: %s expected %s, got %s: %s\n' "$hook" "$expected" "$status" "$output" >&2
        exit 1
    fi
}

bash_payload() {
    jq -nc --arg command "$1" '{tool_name:"Bash",tool_input:{command:$command}}'
}

file_payload() {
    jq -nc --arg path "$1" '{tool_name:"Write",tool_input:{file_path:$path}}'
}

change_policy() {
    jq "$1" "$HOOKS/worktree-policy.json" >"$HOOKS/policy.next"
    mv "$HOOKS/policy.next" "$HOOKS/worktree-policy.json"
}

expect_status 0 worktree-isolation.sh "$(bash_payload 'git --fixture-root=/tmp status')"
expect_status 2 worktree-isolation.sh "$(bash_payload 'git --work-tree=/tmp status')"
expect_status 0 worktree-file-guard.sh "$(file_payload '.loom/work/handoffs/proof.md')"

# Change a temporary installed copy: both token and malformed-command fallback
# must consume the data, rather than silently retaining their old literals.
change_policy '(.git_options[] | select(.flag == "--work-tree") | .flag) = "--fixture-root"'
expect_status 2 worktree-isolation.sh "$(bash_payload 'git --fixture-root=/tmp status')"
expect_status 2 worktree-isolation.sh "$(bash_payload 'git --fixture-root=/tmp "')"
expect_status 0 worktree-isolation.sh "$(bash_payload 'git --work-tree=/tmp status')"
change_policy '.shared_state.handoffs.live_write = "deny"'
expect_status 2 worktree-file-guard.sh "$(file_payload '.loom/work/handoffs/proof.md')"

# Changes to lexical primitives reach both live enforcement paths.
change_policy '.parent_pair = "fixture-parent" | .parent_component = "fixture-component"'
expect_status 2 worktree-isolation.sh "$(bash_payload 'cat fixture-parent')"
expect_status 2 worktree-file-guard.sh "$(file_payload 'src/fixture-component')"

for corruption in missing malformed schema invalid_regex; do
    cp "$SOURCE/worktree-policy.json" "$HOOKS/worktree-policy.json"
    case "$corruption" in
    missing) rm "$HOOKS/worktree-policy.json" ;;
    malformed) printf '{' >"$HOOKS/worktree-policy.json" ;;
    schema) printf '{}' >"$HOOKS/worktree-policy.json" ;;
    invalid_regex) change_policy '.parent_pair = "["' ;;
    esac
    expect_status 2 worktree-isolation.sh "$(bash_payload 'git status')"
    expect_status 2 worktree-file-guard.sh "$(file_payload 'src/main.rs')"
done

echo PASS
