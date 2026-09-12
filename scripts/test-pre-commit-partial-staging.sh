#!/usr/bin/env bash
# test-pre-commit-partial-staging.sh - prove the hook refuses a partial index.
#
# A formatter followed by git add can silently stage work a developer did not
# intend to commit. This runs the checked-in hook against isolated repositories
# with fake formatters, so the partial-index guard is tested without compiling.

set -euo pipefail

tmp=$(mktemp -d "${TMPDIR:-/tmp}/pre-commit-partial.XXXXXX")
[ -n "$tmp" ]
trap 'rm -rf "$tmp"' EXIT

unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_OBJECT_DIRECTORY
unset GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_PREFIX
# Ambient git env from a CI runner or a loom session (which exports
# GIT_CONFIG_COUNT/KEY_n) must not leak into the fixture repos.
unset GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL
unset GIT_CONFIG_PARAMETERS GIT_CONFIG_COUNT
unset GIT_LITERAL_PATHSPECS GIT_GLOB_PATHSPECS GIT_NOGLOB_PATHSPECS GIT_ICASE_PATHSPECS
export GIT_CONFIG_NOSYSTEM=1
export GIT_CONFIG_GLOBAL=/dev/null
export HOME="$tmp/home"
mkdir -p "$HOME" "$tmp/bin"

repo_root=$(cd "$(dirname "$0")/.." && pwd)
hook="$repo_root/loom/.githooks/pre-commit"
export PATH="$tmp/bin:$PATH"

write_fake() {
	local program=$1
	local log_variable=$2

	printf '%s\n' \
		'#!/usr/bin/env bash' \
		'set -euo pipefail' \
		"printf '%s %s\\n' \"\$(basename \"\$0\")\" \"\$*\" >>\"\${$log_variable}\"" \
		'exit 0' >"$tmp/bin/$program"
	chmod +x "$tmp/bin/$program"
}

write_fake cargo PRE_COMMIT_CARGO_LOG
write_fake bunx PRE_COMMIT_BUNX_LOG

create_case_repo() {
	local fixture_name=$1
	shift
	local rust_path

	case_repo="$tmp/$fixture_name"
	for rust_path in "$@"; do
		mkdir -p "$case_repo/$(dirname "$rust_path")"
		printf 'fn fixture() {}\n' >"$case_repo/$rust_path"
	done
	printf '# Fixture\n' >"$case_repo/README.md"
	git -C "$case_repo" init -q
	git -C "$case_repo" config user.name 'Pre-commit Fixture'
	git -C "$case_repo" config user.email 'pre-commit-fixture@example.invalid'
	for rust_path in "$@"; do
		git -C "$case_repo" add -- ":(literal)$rust_path"
	done
	git -C "$case_repo" add -- README.md
	git -C "$case_repo" commit -q --no-verify -m 'Create fixture'
}

prepare_case() {
	local fixture_name=$1
	shift

	create_case_repo "$fixture_name" "$@"
	cargo_log="$tmp/$fixture_name.cargo.log"
	bunx_log="$tmp/$fixture_name.bunx.log"
	out="$tmp/$fixture_name.out"
	: >"$cargo_log"
	: >"$bunx_log"
	: >"$out"
	export PRE_COMMIT_CARGO_LOG="$cargo_log"
	export PRE_COMMIT_BUNX_LOG="$bunx_log"
}

fail() {
	local fixture_name=$1
	local reason=$2
	local output=$3

	printf 'FAIL: %s: %s\n' "$fixture_name" "$reason" >&2
	cat "$output" >&2
	exit 1
}

run_hook() {
	local fixture_repo=$1
	local output=$2

	if (cd "$fixture_repo" && "$hook") >"$output" 2>&1; then
		status=0
	else
		status=$?
	fi
}

snapshot_path() {
	local fixture_repo=$1
	local path=$2
	local snapshot_prefix=$3

	git -C "$fixture_repo" show ":$path" >"$snapshot_prefix.index-before"
	cp "$fixture_repo/$path" "$snapshot_prefix.worktree-before"
}

assert_path_unchanged() {
	local fixture_name=$1
	local fixture_repo=$2
	local path=$3
	local snapshot_prefix=$4
	local output=$5

	git -C "$fixture_repo" show ":$path" >"$snapshot_prefix.index-after"
	cp "$fixture_repo/$path" "$snapshot_prefix.worktree-after"
	if ! cmp -s "$snapshot_prefix.index-before" "$snapshot_prefix.index-after"; then
		fail "$fixture_name" "hook changed index bytes for $path" "$output"
	fi
	if ! cmp -s "$snapshot_prefix.worktree-before" "$snapshot_prefix.worktree-after"; then
		fail "$fixture_name" "hook changed working-tree bytes for $path" "$output"
	fi
}

assert_nonzero_status() {
	local fixture_name=$1
	local output=$2

	if [ "$status" -eq 0 ]; then
		fail "$fixture_name" 'hook exited 0 for a partially staged path' "$output"
	fi
}

assert_zero_status() {
	local fixture_name=$1
	local output=$2

	if [ "$status" -ne 0 ]; then
		fail "$fixture_name" "hook exited $status; expected 0" "$output"
	fi
}

assert_path_reported() {
	local fixture_name=$1
	local path=$2
	local output=$3

	if ! grep -F -x -e "$path" "$output" >/dev/null; then
		fail "$fixture_name" "hook output did not name $path" "$output"
	fi
}

assert_tools_not_invoked() {
	local fixture_name=$1
	local output=$2

	if [ -s "$cargo_log" ]; then
		fail "$fixture_name" 'cargo ran before the partial-index guard' "$output"
	fi
	if [ -s "$bunx_log" ]; then
		fail "$fixture_name" 'bunx ran before the partial-index guard' "$output"
	fi
}

assert_normal_flow() {
	local fixture_name=$1
	local output=$2

	if ! grep -F -x -e 'cargo fmt' "$cargo_log" >/dev/null; then
		fail "$fixture_name" 'cargo fmt was not invoked' "$output"
	fi
	if ! grep -F -x -e 'cargo test --quiet --test maintainability' "$cargo_log" >/dev/null; then
		fail "$fixture_name" 'maintainability test was not invoked' "$output"
	fi
	if [ ! -s "$bunx_log" ]; then
		fail "$fixture_name" 'bunx was not invoked for tracked Markdown' "$output"
	fi
}

assert_partial_refused() {
	local fixture_name=$1
	local path=$2
	local snapshot_prefix=$3

	snapshot_path "$case_repo" "$path" "$snapshot_prefix"
	run_hook "$case_repo" "$out"
	assert_nonzero_status "$fixture_name" "$out"
	assert_path_reported "$fixture_name" "$path" "$out"
	assert_tools_not_invoked "$fixture_name" "$out"
	assert_path_unchanged "$fixture_name" "$case_repo" "$path" "$snapshot_prefix" "$out"
}

run_partially_staged_case() {
	local fixture_name=$1
	local path=$2
	local snapshot_prefix="$tmp/$fixture_name"

	prepare_case "$fixture_name" "$path"
	printf 'fn fixture() { 1; }\n' >"$case_repo/$path"
	git -C "$case_repo" add -- "$path"
	printf 'fn fixture() { 2; }\n' >"$case_repo/$path"
	assert_partial_refused "$fixture_name" "$path" "$snapshot_prefix"
}

run_renamed_path_case() {
	local fixture_name=renamed-path
	local source_path=loom/fixture.rs
	local path=loom/renamed.rs
	local snapshot_prefix="$tmp/$fixture_name"

	prepare_case "$fixture_name" "$source_path"
	git -C "$case_repo" mv "$source_path" "$path"
	printf 'fn renamed_fixture() { 1; }\n' >"$case_repo/$path"
	assert_partial_refused "$fixture_name" "$path" "$snapshot_prefix"
}

run_fully_staged_case() {
	local fixture_name=fully-staged
	local path=loom/fixture.rs

	prepare_case "$fixture_name" "$path"
	printf 'fn fixture() { 1; }\n' >"$case_repo/$path"
	git -C "$case_repo" add -- "$path"
	run_hook "$case_repo" "$out"
	assert_zero_status "$fixture_name" "$out"
	assert_normal_flow "$fixture_name" "$out"
}

run_unstaged_only_case() {
	local fixture_name=unstaged-only
	# staged_path has glob characters so a glob-pathspec re-add (rather than a
	# literal one) would also stage its unstaged sibling.
	local staged_path='loom/[ab].rs'
	local sibling_path=loom/a.rs
	local unstaged_path=README.md

	prepare_case "$fixture_name" "$staged_path" "$sibling_path"
	printf 'fn fixture() { 1; }\n' >"$case_repo/$staged_path"
	git -C "$case_repo" add -- ":(literal)$staged_path"
	printf 'fn a() { 1; }\n' >"$case_repo/$sibling_path"
	printf '# Unstaged fixture\n' >"$case_repo/$unstaged_path"
	run_hook "$case_repo" "$out"
	assert_zero_status "$fixture_name" "$out"
	assert_normal_flow "$fixture_name" "$out"
	if grep -F -x -e "$sibling_path" "$out" >/dev/null; then
		fail "$fixture_name" "$sibling_path was reported despite being only unstaged" "$out"
	fi
	if grep -F -x -e "$unstaged_path" "$out" >/dev/null; then
		fail "$fixture_name" "$unstaged_path was reported despite being only unstaged" "$out"
	fi
	if ! git -C "$case_repo" diff --cached --quiet -- "$sibling_path"; then
		fail "$fixture_name" "$sibling_path was added to the index" "$out"
	fi
	if ! git -C "$case_repo" diff --cached --quiet -- "$unstaged_path"; then
		fail "$fixture_name" "$unstaged_path was added to the index" "$out"
	fi
}

run_two_partial_paths_case() {
	local fixture_name=two-partial-paths-with-quoting
	local path_a=loom/fixture.rs
	# git C-quotes this path (embedded double quote) when -z is absent; it
	# proves the enumeration must stay NUL-delimited.
	local path_b='loom/has space "quoted".rs'
	local prefix_a="$tmp/$fixture_name.a"
	local prefix_b="$tmp/$fixture_name.b"

	prepare_case "$fixture_name" "$path_a" "$path_b"
	printf 'fn fixture() { 1; }\n' >"$case_repo/$path_a"
	git -C "$case_repo" add -- ":(literal)$path_a"
	printf 'fn fixture() { 2; }\n' >"$case_repo/$path_a"

	printf 'fn quoted() { 1; }\n' >"$case_repo/$path_b"
	git -C "$case_repo" add -- ":(literal)$path_b"
	printf 'fn quoted() { 2; }\n' >"$case_repo/$path_b"

	snapshot_path "$case_repo" "$path_a" "$prefix_a"
	snapshot_path "$case_repo" "$path_b" "$prefix_b"
	run_hook "$case_repo" "$out"
	assert_nonzero_status "$fixture_name" "$out"
	assert_path_reported "$fixture_name" "$path_a" "$out"
	assert_path_reported "$fixture_name" "$path_b" "$out"
	assert_tools_not_invoked "$fixture_name" "$out"
	assert_path_unchanged "$fixture_name" "$case_repo" "$path_a" "$prefix_a" "$out"
	assert_path_unchanged "$fixture_name" "$case_repo" "$path_b" "$prefix_b" "$out"
}

run_hooks_path_commit_case() {
	local fixture_name=hooks-path-commit
	local path=loom/fixture.rs
	local snapshot_prefix="$tmp/$fixture_name"
	local head_before
	local head_after

	prepare_case "$fixture_name" "$path"
	git -C "$case_repo" config core.hooksPath "$repo_root/loom/.githooks"
	printf 'fn fixture() { 1; }\n' >"$case_repo/$path"
	git -C "$case_repo" add -- "$path"
	printf 'fn fixture() { 2; }\n' >"$case_repo/$path"
	snapshot_path "$case_repo" "$path" "$snapshot_prefix"
	head_before=$(git -C "$case_repo" rev-parse HEAD)

	# GIT_LITERAL_PATHSPECS=1 proves the hook clears inherited pathspec settings
	# when git itself runs it, as well as when invoked directly.
	if (cd "$case_repo" && GIT_LITERAL_PATHSPECS=1 git commit -q -m 'Attempt partial commit') >"$out" 2>&1; then
		status=0
	else
		status=$?
	fi

	assert_nonzero_status "$fixture_name" "$out"
	assert_path_reported "$fixture_name" "$path" "$out"
	assert_tools_not_invoked "$fixture_name" "$out"
	assert_path_unchanged "$fixture_name" "$case_repo" "$path" "$snapshot_prefix" "$out"
	head_after=$(git -C "$case_repo" rev-parse HEAD)
	if [ "$head_before" != "$head_after" ]; then
		fail "$fixture_name" 'HEAD moved despite the hook refusing the commit' "$out"
	fi
}

run_partially_staged_case partial-path loom/fixture.rs
run_fully_staged_case
run_two_partial_paths_case
run_unstaged_only_case
run_renamed_path_case
run_hooks_path_commit_case

printf 'test-pre-commit-partial-staging: 6 cases passed\n'
