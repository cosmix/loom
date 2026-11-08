#!/usr/bin/env bash
# fixture_base - print the physical directory a test creates its fixture root
# in. That is TMPDIR, unless TMPDIR lies under the literal /tmp/claude-<uid>
# (macOS /private/tmp), the Bash tool's own TMPDIR: worktree-file-guard.sh lets
# the Read tool open plain files of ours there, so a fixture under it reads as
# harness temp output and no block can be tested. Then the base is
# <root>/loom-fixtures/<uuid>, which allow_tmpdir treats as another session's
# directory and keeps closed. A base inside the checkout would not do: a
# checkout under `.worktrees/<id>` puts every fixture cwd in that worktree.
# Payloads in these tests never carry this uuid as their session_id.
FIXTURE_CLOSED_SESSION=00000000-0000-4000-8000-000000000000

fixture_base() {
	local base uid_root
	uid_root="/tmp/claude-$(id -u)"
	base=$(cd "${TMPDIR:-/tmp}" && pwd -P)
	case "$base/" in
	"$uid_root"/* | "/private$uid_root"/*)
		[[ "$base" != /private/* ]] || uid_root="/private$uid_root"
		base="$uid_root/loom-fixtures/$FIXTURE_CLOSED_SESSION"
		mkdir -p "$base"
		;;
	esac
	printf '%s\n' "$base"
}
