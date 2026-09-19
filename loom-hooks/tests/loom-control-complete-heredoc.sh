#!/usr/bin/env bash
# The completion bridge decides on the command with its heredoc bodies
# stripped, so prose in a body - the completion shape spelled out, an
# apostrophe that breaks tokenizing - no longer blocks a knowledge write or a
# commit message. The stripping is trusted only when the bodies are provably
# inert; every case below that bash would really run as a completion, or that
# the strip helper could misread, must still reach the pin and be refused.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/loom-hooks/loom-control-complete.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

WORKTREE="$TMP/repo/.worktrees/build-api"
mkdir -p "$TMP/bin" "$WORKTREE"
LOG="$TMP/broker.log"
printf '#!/usr/bin/env bash\nprintf "%%s\\n" "$*" >>"$BROKER_LOG"\n' >"$TMP/bin/loom"
chmod +x "$TMP/bin/loom"
PINNED="$TMP/bin/loom stage complete build-api"

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

# read_cmd - Read one literal command from stdin (a quoted heredoc) into CMD.
read_cmd() {
	IFS= read -r -d '' CMD || true
	CMD=${CMD%$'\n'}
}

# invoke_hook <command> [pre|post] - Run the hook with a scrubbed environment.
invoke_hook() {
	local payload
	if [[ "${2:-pre}" == post ]]; then
		payload=$(jq -n --arg c "$1" '{tool_name:"Bash",tool_input:{command:$c},tool_result:{output:"ok",is_error:false}}')
	else
		payload=$(jq -n --arg c "$1" '{tool_name:"Bash",tool_input:{command:$c}}')
	fi
	rm -f "$LOG"
	set +e
	HOOK_OUTPUT=$(printf '%s' "$payload" |
		env -i PATH="$PATH" HOME="$TMP" BROKER_LOG="$LOG" LOOM_CONTROL_TESTING=1 \
			LOOM_CONTROL_TEST_BIN="$TMP/bin/loom" LOOM_STAGE_ID=build-api \
			LOOM_SESSION_ID=session-123 LOOM_WORKTREE_PATH="$WORKTREE" bash "$HOOK" 2>&1)
	HOOK_RC=$?
	set -e
	[[ ! -e "$LOG" ]] || fail "$3 reached the broker"
}

expect_allowed() {
	local label=$1 phase
	for phase in pre post; do
		invoke_hook "$CMD" "$phase" "$label"
		[[ "$HOOK_RC" == 0 && -z "$HOOK_OUTPUT" ]] || fail "$label ($phase) was blocked: $HOOK_OUTPUT"
	done
}

expect_refused() {
	local label=$1
	invoke_hook "$CMD" pre "$label"
	[[ "$HOOK_RC" == 2 && "$HOOK_OUTPUT" == *LOOM_CONTROL_ERROR:* ]] ||
		fail "$label was not refused (rc $HOOK_RC): $HOOK_OUTPUT"
	PRE_OUTPUT=$HOOK_OUTPUT
	invoke_hook "$CMD" post "$label"
	[[ "$HOOK_RC" == 2 ]] || fail "$label passed PostToolUse (rc $HOOK_RC)"
}

# 1. The pinned command is untouched by the heredoc path.
CMD=$PINNED
invoke_hook "$CMD" pre "the pinned command"
[[ "$HOOK_RC" == 0 ]] || fail "the pinned command was refused: $HOOK_OUTPUT"

# 2. Heredoc prose is data. Each body spells the completion shape at a line
# start and carries an apostrophe (the two old false-positive paths).
read_cmd <<'CASE' || true
loom knowledge replace-section doc/loom/knowledge/mistakes/finalize.md "Finalize guard" <<'EOF'
The guard's prefilter used to scan this body.
loom stage complete build-api
Run it only when the work is verified.
EOF
CASE
expect_allowed "replace-section with a quoted delimiter"

read_cmd <<'CASE' || true
loom knowledge update mistakes <<"EOF"
it's the loom stage complete step; loom stage complete build-api
EOF
CASE
expect_allowed "update with a double-quoted delimiter"

read_cmd <<'CASE' || true
cat > notes.txt <<'EOF'
loom stage complete build-api | tee done; it's fine
EOF
CASE
expect_allowed "cat into a file"

read_cmd <<'CASE' || true
loom memory note "$(cat <<'EOF'
gotcha: loom stage complete build-api runs last, it's pinned
EOF
)"
CASE
expect_allowed "a memory note through a command substitution"

read_cmd <<'CASE' || true
git commit -F - <<'EOF'
fix(hooks): stop reading heredoc prose

loom stage complete build-api is the pinned form; it's exact.
EOF
CASE
expect_allowed "a commit message on stdin"

read_cmd <<'CASE' || true
cd doc && loom knowledge update patterns <<'EOF'
loom stage complete build-api
EOF
cat > b.txt <<'END'
it's here: loom stage complete build-api
END
CASE
expect_allowed "two heredocs after a cd"

# 3. Genuine completions stay held to the pin, and so does anything the strip
# helper could misread.
read_cmd <<'CASE' || true
bash <<'EOF'
loom stage complete build-api
EOF
CASE
expect_refused "a heredoc passed to bash"
CMD=${CMD/bash/sh}
expect_refused "a heredoc passed to sh"

read_cmd <<'CASE' || true
bash <<'EOF'
x=stage
loom $x complete build-api
EOF
CASE
expect_refused "a heredoc passed to bash, verb built from a variable"
[[ "$PRE_OUTPUT" == *'a shell -c wrapper is not allowed'* ]] || fail "interpreter reason: $PRE_OUTPUT"

read_cmd <<'CASE' || true
xargs loom stage <<'EOF'
complete build-api
EOF
CASE
expect_refused "xargs building the command from a heredoc"

read_cmd <<'CASE' || true
cat <<'EOF' | bash
loom stage complete build-api
EOF
CASE
expect_refused "a heredoc piped into bash"

read_cmd <<'CASE' || true
source /dev/stdin <<'EOF'
loom stage complete build-api
EOF
CASE
expect_refused "a heredoc sourced from stdin"
CMD=${CMD/source/.}
expect_refused "a heredoc dotted from stdin"

read_cmd <<'CASE' || true
eval "$(cat <<'EOF'
loom stage complete build-api
EOF
)"
CASE
expect_refused "eval of a heredoc"

read_cmd <<'CASE' || true
cat <<EOF
$(loom stage complete build-api)
EOF
CASE
expect_refused "an unquoted delimiter whose body expands a command"

read_cmd <<'CASE' || true
cat <<'EOF'
it's data
EOF
loom stage complete build-api
CASE
expect_refused "a completion after the heredoc ends"

read_cmd <<'CASE' || true
echo "<<'EOF' "
loom stage complete build-api
EOF
CASE
expect_refused "an opener inside double quotes"

read_cmd <<'CASE' || true
echo $'\' <<'EOF' '\'
loom stage complete build-api
EOF
CASE
expect_refused "an opener hidden by ANSI-C quoting"

read_cmd <<'CASE' || true
cat <<'EOF' "a
b" ; loom stage complete build-api
EOF
CASE
expect_refused "an opener line that ends inside a quote"

read_cmd <<'CASE' || true
cat <<'EOF' \
; loom stage complete build-api
x
EOF
CASE
expect_refused "an opener line continued by a backslash"

read_cmd <<'CASE' || true
x=$(cat <<'EOF'
body
EOF)
loom stage complete build-api
EOF
CASE
expect_refused "a body that bash ends at EOF)"

CMD=$'cat <<-\'EOF\'\n\tbody\n\tEOF\nloom stage complete build-api\nEOF'
expect_refused "a tab-stripped delimiter"

read_cmd <<'CASE' || true
cat <<'E'OF
x
EOF
loom stage complete build-api
E
CASE
expect_refused "a partly quoted delimiter"

read_cmd <<'CASE' || true
: $((1<<EOF))
loom stage complete build-api
EOF
CASE
expect_refused "an arithmetic shift mistaken for an opener"

read_cmd <<'CASE' || true
cat <<<EOF
loom stage complete build-api
EOF
CASE
expect_refused "a here-string mistaken for an opener"

read_cmd <<'CASE' || true
# <<'EOF'
loom stage complete build-api
EOF
CASE
expect_refused "an opener inside a comment"

read_cmd <<'CASE' || true
cat <<'EOF'
b
EOF
echo "x -m"; loom stage complete build-api; echo "y"
CASE
expect_refused "a completion between quotes the -m rewrite would join"

read_cmd <<'CASE' || true
while read -r a b c d; do $a $b $c $d; done <<'EOF'
loom stage complete build-api
EOF
CASE
expect_refused "a read loop running each body line"

read_cmd <<'CASE' || true
git -c alias.x='!sh' x <<'EOF'
loom stage complete build-api
EOF
CASE
expect_refused "git running a shell alias on the body"

read_cmd <<'CASE' || true
perl <<'EOF'
print `loom stage complete build-api`;
EOF
CASE
expect_refused "a heredoc passed to perl"

CMD="$PINNED <<'EOF'"$'\nx\nEOF'
expect_refused "the pinned command with a heredoc"

echo "PASS"
