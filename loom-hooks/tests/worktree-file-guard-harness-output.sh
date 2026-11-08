#!/usr/bin/env bash
# worktree-file-guard-harness-output.sh - the Read tool may open this session's
# harness tool output (~/.claude/projects/<slug>/<session>/tool-results/<file>)
# and plain files of ours under the literal /tmp/claude-<uid>. Everything around
# them stays blocked: another session, a symlinked or hard-linked entry, a
# foreign-owned directory, any write, Grep and Glob, a `..` component, another
# session's uuid directory, and a symlink below the root. Every block names its
# rule on the first stderr line and lands in the stage block ledger.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK="$(cd "$SCRIPT_DIR/.." && pwd)/worktree-file-guard.sh"
UID_N=$(id -u)
UID_ROOT="/tmp/claude-$UID_N"

# Fixtures must not read as harness temp output (_fixture_base.sh).
source "$SCRIPT_DIR/_fixture_base.sh"
TMP=$(mktemp -d "$(fixture_base)/loom-harness-output.XXXXXX")
TMP=$(cd "$TMP" && pwd -P)
BSD_DIR=""
PROJECT=""
CREATED_ROOT=false
cleanup() {
	rm -rf "$TMP" ${BSD_DIR:+"$BSD_DIR"} ${PROJECT:+"$PROJECT"}
	rm -f "$UID_ROOT/loom-harness-$$.log" "$UID_ROOT/loom-harness-link-$$" "$UID_ROOT/loom-harness-leaf-$$"
	[[ "$CREATED_ROOT" == false ]] || rmdir "$UID_ROOT" 2>/dev/null || true
}
trap cleanup EXIT
PATH_PREFIX="$PATH"
if [[ "${LOOM_HOOK_TEST_BSD:-}" == "1" ]]; then
	source "$SCRIPT_DIR/_bsd_path.sh"
	BSD_DIR=$(bsd_shim_dir)
	PATH_PREFIX="$BSD_DIR:$PATH"
fi

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

SID="0f0e0d0c-1111-2222-3333-$(printf '%012d' "$$")"
OTHER_SID="0f0e0d0c-4444-5555-6666-$(printf '%012d' "$$")"
WORKTREE="$TMP/repo/.worktrees/build-api"
OUTSIDE="$TMP/outside"
HOME_DIR="$TMP/home"
PROJECTS="$HOME_DIR/.claude/projects"
RESULTS="$PROJECTS/p/$SID/tool-results"
mkdir -p "$WORKTREE/.loom" "$TMP/repo/.loom/work" "$TMP/tmpdir" "$TMP/work" \
	"$OUTSIDE" "$RESULTS" "$PROJECTS/p/$OTHER_SID/tool-results"
chmod 700 "$TMP/tmpdir"
ln -s ../../../.loom/work "$WORKTREE/.loom/work"
ln -s "$HOME_DIR" "$TMP/homelink"
ln -s "$PROJECTS/p" "$PROJECTS/q"
printf 'secret\n' >"$OUTSIDE/secret"
printf 'tool output\n' >"$RESULTS/b1.txt"
printf 'other session\n' >"$PROJECTS/p/$OTHER_SID/tool-results/b1.txt"
ln -s /etc/hostname "$RESULTS/hostname.txt"
ln -s "$OUTSIDE/secret" "$RESULTS/secret.txt"
ln "$OUTSIDE/secret" "$RESULTS/hard.txt"

RUN_HOME="$HOME_DIR"
RUN_ENV=()
# run_hook <tool> <path> [payload] - one guard call from inside the worktree.
run_hook() {
	local tool=$1 path=$2 payload=${3-} field=file_path
	case "$tool" in Glob | Grep) field=path ;; esac
	if [[ -z "$payload" ]]; then
		payload=$(jq -nc --arg t "$tool" --arg f "$field" --arg p "$path" --arg s "$SID" \
			'{tool_name: $t, session_id: $s,
			  tool_input: {($f): $p, pattern: "*", content: "x", old_string: "x", new_string: "y"}}')
	fi
	set +e
	OUTPUT=$(cd "$WORKTREE" && printf '%s' "$payload" |
		env -i PATH="${SHIM_PATH:-}$PATH_PREFIX" HOME="$RUN_HOME" \
			${RUN_ENV[@]+"${RUN_ENV[@]}"} bash "$HOOK" 2>&1)
	RC=$?
	set -e
}

expect_allowed() {
	run_hook "$@"
	[[ "$RC" == 0 ]] || fail "$1 $2 was blocked: $OUTPUT"
}

# expect_blocked <rule> <tool> <path> [payload] - exit 2, with the rule tag on
# the first non-empty stderr line.
expect_blocked() {
	local rule=$1 first
	shift
	run_hook "$@"
	first=$(printf '%s\n' "$OUTPUT" | grep -m1 -v '^[[:space:]]*$' || true)
	[[ "$RC" == 2 ]] || fail "$1 $2 was not blocked (rc $RC): $OUTPUT"
	[[ "$first" == "[worktree-file-guard/$rule]"* ]] || fail "$1 $2: first line lacks [$rule]: $OUTPUT"
}

# 1. This session's tool-results, with HOME real or a symlink to it.
RUN_ENV=(TMPDIR="$TMP/tmpdir")
expect_allowed Read "$RESULTS/b1.txt"
expect_allowed Read "~/.claude/projects/p/$SID/tool-results/b1.txt"
RUN_HOME="$TMP/homelink"
expect_allowed Read "$TMP/homelink/.claude/projects/p/$SID/tool-results/b1.txt"
expect_allowed Read "$RESULTS/b1.txt"
RUN_HOME="$HOME_DIR"

# 2. Everything around it stays blocked.
expect_blocked outside-worktree Read "$PROJECTS/p/$OTHER_SID/tool-results/b1.txt"
expect_blocked symlink-leaf Read "$RESULTS/hostname.txt"
expect_blocked symlink-leaf Read "$RESULTS/secret.txt"
expect_blocked outside-worktree Read "$RESULTS/hard.txt"
expect_blocked outside-worktree Read "$PROJECTS/q/$SID/tool-results/b1.txt"
expect_blocked outside-worktree Read "$RESULTS"
expect_blocked outside-worktree Write "$RESULTS/b1.txt"
expect_blocked outside-worktree Edit "$RESULTS/b1.txt"
expect_blocked outside-worktree Grep "$RESULTS"
expect_blocked outside-worktree Glob "$RESULTS"
NO_SESSION=$(jq -nc --arg p "$RESULTS/b1.txt" '{tool_name: "Read", tool_input: {file_path: $p}}')
expect_blocked outside-worktree Read "$RESULTS/b1.txt" "$NO_SESSION"

# 3. A directory owned by another uid: a stat stub in $TMP/shim-<dir> reports a
# foreign owner for directories named <dir> alone.
REAL_STAT=$(PATH="$PATH_PREFIX" command -v stat)
for dir in tool-results client; do
	mkdir -p "$TMP/shim-$dir"
	printf '#!/usr/bin/env bash\nfor a in "$@"; do case "$a" in */%s) echo 424242; exit 0 ;; esac; done\nexec %q "$@"\n' \
		"$dir" "$REAL_STAT" >"$TMP/shim-$dir/stat"
	chmod +x "$TMP/shim-$dir/stat"
done
SHIM_PATH="$TMP/shim-tool-results:" expect_blocked outside-worktree Read "$RESULTS/b1.txt"

# 4. A block is recorded in the stage block ledger, and an unparsable payload
# is blocked as bad-payload with tool `unknown`.
LEDGER="$TMP/work/heartbeat/s.blocks.jsonl"
RUN_ENV=(TMPDIR="$TMP/tmpdir" LOOM_WORK_DIR="$TMP/work" LOOM_STAGE_ID=s LOOM_SESSION_ID=x)
expect_blocked symlink-leaf Read "$RESULTS/hostname.txt"
expect_blocked bad-payload Read "" "not json"
[[ "$(wc -l <"$LEDGER" | tr -d '[:space:]')" == 2 ]] || fail "ledger rows: $(cat "$LEDGER")"
jq -e -s '.[0].rule == "symlink-leaf" and .[0].tool == "Read" and .[0].guard == "worktree-file-guard"
	and .[1].rule == "bad-payload" and .[1].tool == "unknown" and .[1].subject == ""' \
	"$LEDGER" >/dev/null || fail "ledger content: $(cat "$LEDGER")"
RUN_ENV=(TMPDIR="$TMP/tmpdir")

# 5. /tmp/claude-<uid>, set up as worktree-file-guard-scratchpad.sh does.
if [[ ! -d "$UID_ROOT" ]]; then
	mkdir -m 700 "$UID_ROOT" && CREATED_ROOT=true
fi
if ! PROJECT=$(mktemp -d "$UID_ROOT/loom-hooktest-project.XXXXXX"); then
	PROJECT=""
	# Seatbelt cannot give loom's confined runner a private /tmp, so there the
	# session's own root exists read-only. Every other failure still fails.
	if [[ -d "$UID_ROOT" && ! -w "$UID_ROOT" ]]; then
		echo "PASS (SKIP /tmp/claude-$UID_N cases: $UID_ROOT not writable)"
		exit 0
	fi
	fail "cannot create a project directory under $UID_ROOT"
fi
LOG="$UID_ROOT/loom-harness-$$.log"
printf 'log\n' >"$LOG"
mkdir -p "$PROJECT/$SID" "$PROJECT/$OTHER_SID/scratchpad" "$PROJECT/client"
printf 'mine\n' >"$PROJECT/$SID/notes.txt"
printf 'theirs\n' >"$PROJECT/$OTHER_SID/scratchpad/x"
printf 'fixture\n' >"$PROJECT/client/f.txt"
printf 'pair\n' >"$PROJECT/client/a.txt"
ln "$PROJECT/client/a.txt" "$PROJECT/client/b.txt"
ln -s "$PROJECT" "$UID_ROOT/loom-harness-link-$$"
ln -s "$OUTSIDE/secret" "$UID_ROOT/loom-harness-leaf-$$"

RUN_ENV=()
expect_allowed Read "$LOG"
RUN_ENV=(TMPDIR="$TMP/tmpdir")
expect_allowed Read "$LOG"
expect_allowed Read "$PROJECT/$SID/notes.txt"
expect_allowed Read "$PROJECT/client/f.txt"

expect_blocked outside-worktree Write "$LOG"
expect_blocked outside-worktree Edit "$LOG"
expect_blocked outside-worktree Write "$UID_ROOT/loom-harness-new-$$.log"
expect_blocked outside-worktree Grep "$UID_ROOT"
expect_blocked outside-worktree Glob "$UID_ROOT"
expect_blocked outside-worktree Grep "$PROJECT/$SID"
expect_blocked parent-component Read "$UID_ROOT/../x"
expect_blocked outside-worktree Read "$PROJECT/$OTHER_SID/scratchpad/x"
expect_blocked outside-worktree Read "$UID_ROOT/loom-harness-link-$$/client/f.txt"
expect_blocked symlink-leaf Read "$UID_ROOT/loom-harness-leaf-$$"
expect_blocked outside-worktree Read "$PROJECT/client/a.txt"
expect_blocked outside-worktree Read "$PROJECT/client"
expect_blocked outside-worktree Read "/tmp/claude-424242/x.log"
SHIM_PATH="$TMP/shim-client:" expect_blocked outside-worktree Read "$PROJECT/client/f.txt"

echo "PASS"
