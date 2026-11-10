#!/usr/bin/env bash
# subagent-verify-guard.sh - PreToolUse hook blocking full-suite runs by SUBAGENTS
#
# Verification belongs to the stage's gate, never to an implementer: a
# subagent running the full build/test/lint/typecheck suite burns wall-clock
# and tokens and surfaces failures it was never asked to own. Project-wide
# runners are blocked at the tool boundary, narrowly-scoped runs (a filter,
# one test target, a path) allowed.
#
# The MAIN AGENT is affected in exactly one case: a live loom STANDARD stage
# session, where it may not run an unfiltered TEST SUITE (the full suite runs
# only through `loom check`). Build, clippy, fmt, tsc, eslint and make stay allowed
# for it. Everywhere else (no live session, any other stage type) it is never
# blocked. The hook acts only on a live loom session: `loom_is_subagent`
# (loom-hooks/_common.sh) and `live_loom_session` below both gate on
# LOOM_MAIN_AGENT_PID being set and a live process-tree ancestor FIRST, ALWAYS,
# because this hook installs globally at ~/.claude/hooks/loom/, so that
# precondition is the only thing scoping the block to a loom stage session
# rather than every Claude Code session on the machine. Only once that gate
# passes does the hook's JSON payload (captured below as $INPUT_JSON) decide
# main-vs-subagent outright: `.agent_type` / `.transcript_path`, which the
# caller cannot forge, decide "subagent" or "main" when present - a Task-tool
# subagent's own `.agent_type` names it even though it runs IN-PROCESS with no
# separate Claude process to find (that subagent trivially satisfies the
# live-ancestor gate too, since it runs inside the very claude process
# LOOM_MAIN_AGENT_PID names), and a Bash-tool shell's cmdline (which often
# mentions a ~/.claude/ path, e.g. sourcing a shell-snapshot file) can no
# longer be mistaken for an intervening Claude process. Only when the payload
# answers neither field does the hook fall back to counting Claude processes
# between the caller and LOOM_MAIN_AGENT_PID. One caller is carved out below:
# any subagent of an integration-verify stage, because that stage keeps its own
# gate (its orchestrator and reviewers run the full suite through
# `loom check integration-verify`, which is doctrine in the subagent preamble,
# not enforced here). There is deliberately NO escape-hatch env var - an
# opt-out would defeat the whole purpose (commit-filter.sh already treats
# unsetting the detection gate as evasion).
#
# SECURITY NOTE (best-effort, defense-in-depth): matching is token-based, not a
# shell parser, so a runner hidden in a quoted string (`bash -c "cargo test"`)
# or built by substitution is not seen. Rules are CONSERVATIVE - an unrecognised
# command is allowed, since a false block strands a subagent mid-task; the
# durable guarantee is doctrinal, and this hook only raises the cost.
# The runner matchers (check_command and its helpers) live in
# _verify_matchers.sh, sourced below; block() and the walk loop stay here.
#
# Input: JSON from stdin - {"tool_name": "Bash", "tool_input": {"command": ...}}
# Exit codes: 0 = allow, 2 = block with guidance on stderr. A missing jq or
# _verify_matchers.sh exits 1: the call proceeds with a warning. A missing
# _common.sh, _bounded.sh or any helper _common.sh sources also exits 1, through
# `set -e` with bash's own message.

set -euo pipefail

source "$(dirname "$0")/_common.sh"
# Advisory on its own breakage: without jq the call proceeds with a warning.
loom_warn_no_jq "subagent-verify-guard.sh"

source "$(dirname "${BASH_SOURCE[0]}")/_bounded.sh"
INPUT_JSON=$(loom_read_input 1 2>/dev/null || true)

TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
TOOL_INPUT=$(echo "$INPUT_JSON" | jq -r '.tool_input // empty' 2>/dev/null || true)

if [[ "$TOOL_NAME" == "Bash" ]]; then
	COMMAND=$(echo "$TOOL_INPUT" | jq -r '.command // empty' 2>/dev/null || echo "$TOOL_INPUT")
else
	COMMAND=""
fi

if [[ "$TOOL_NAME" != "Bash" ]] || [[ -z "$COMMAND" ]]; then
	exit 0
fi

# === CALLER ROLE ===
# live_loom_session - the loom-session gate of `loom_is_subagent`, alone: it
# separates a MAIN agent in a live session from no session at all, which
# `loom_is_subagent` reports identically (non-zero).
live_loom_session() {
	local main_pid="${LOOM_MAIN_AGENT_PID:-}" ancestry=0
	[[ -n "$main_pid" ]] || return 1
	is_ancestor "$main_pid" || ancestry=$?
	[[ $ancestry -ne 1 ]] || return 1
	# 2 = no process information here; a still-signalable pid satisfies the gate
	[[ $ancestry -ne 2 ]] || kill -0 "$main_pid" 2>/dev/null
}

ROLE=subagent
if ! loom_is_subagent "$INPUT_JSON"; then
	live_loom_session || exit 0
	ROLE=main
fi

# === STAGE LOOKUP ===
# WHY LOOM_STAGE_ID IS SAFE HERE AND ONLY HERE: LOOM_STAGE_ID leaks into plain
# Claude Code sessions (knowledge mistakes.md, "Worktree-Isolation Hooks Gated
# on LOOM_STAGE_ID"), so it must never be an activation gate. It is not one
# here: the role check above ALWAYS proves LOOM_MAIN_AGENT_PID is a LIVE
# ancestor FIRST, so we are demonstrably inside a live loom session by the
# time this line runs. The integration-verify carve-out only ever RELAXES the
# hook; the main agent is guarded only when the file declares `standard`.
#
# AMBIGUITY IS FAIL-SAFE: stage files carry a depth prefix (02-<id>.md), so the
# glob can match more than once. Taking the first would let a planted
# `00-<id>.md` claiming integration-verify beat the real file, and a hook with
# no escape hatch must not ship a cheap one. Preferring the match whose `id:`
# agrees is no defence either - whoever writes the decoy writes its `id:`. So:
# exactly ONE match may be consulted; duplicates, a missing file, or one not
# declaring the type all fall through to the checks below (subagent) or to
# allow (main agent).
STAGE_ID="${LOOM_STAGE_ID:-}"
WORK_DIR="${LOOM_WORK_DIR:-}"
STAGE_FILE=""
if [[ -n "$STAGE_ID" && -n "$WORK_DIR" ]]; then
	STAGE_MATCHES=()
	for candidate in "${WORK_DIR}/stages/"*"-${STAGE_ID}.md"; do
		if [[ -f "$candidate" ]]; then STAGE_MATCHES+=("$candidate"); fi
	done
	if [[ ${#STAGE_MATCHES[@]} -eq 1 ]]; then
		STAGE_FILE="${STAGE_MATCHES[0]}"
	elif [[ ${#STAGE_MATCHES[@]} -eq 0 ]]; then
		STAGE_FILE="${WORK_DIR}/stages/${STAGE_ID}.md"
	else
		loom_debug "DEBUG: ${#STAGE_MATCHES[@]} stage files match $STAGE_ID - no stage type"
		STAGE_FILE=""
	fi
fi

# stage_declares <type> - the single consulted stage file declares stage_type.
stage_declares() {
	[[ -n "$STAGE_FILE" && -r "$STAGE_FILE" ]] &&
		grep -qE "^stage_type:[[:space:]]*$1" "$STAGE_FILE" 2>/dev/null
}

if [[ "$ROLE" == main ]]; then
	stage_declares standard || exit 0
elif stage_declares integration-verify; then
	loom_debug "DEBUG: integration-verify stage $STAGE_ID - full-suite runs allowed"
	exit 0
fi

# Strip embedded content (heredoc bodies, -m messages) BEFORE matching, so
# `git commit -m "add cargo test for X"` is not treated as a test run.
STRIPPED=$(strip_embedded_content "$COMMAND")
if [[ -z "$STRIPPED" ]]; then
	exit 0
fi

# Normalize before splitting. IFS splitting breaks on WHITESPACE only, so a
# metacharacter written without surrounding spaces stays glued to its neighbour
# and no separator is ever seen: `echo hi; cargo test` leaves the runner at a
# non-command position and `cargo test;` hides the subcommand behind `test;`.
# A newline separates commands just as `;` does, but splitting eats it. A lone
# `&` is deliberately NOT padded - that would split `2>&1` into `2>` and `1`,
# and `1` would read as a test filter, reopening a bypass closed here. The `\&`
# below is required: an unescaped `&` in a ${//} replacement expands to the text
# that matched, so ` && ` would produce ` &&&& `.
STRIPPED=${STRIPPED//$'\n'/;}
STRIPPED=${STRIPPED//&&/ \&\& }
STRIPPED=${STRIPPED//;/ ; }
STRIPPED=${STRIPPED//|/ | }
STRIPPED=${STRIPPED//\(/ \( }
STRIPPED=${STRIPPED//\)/ \) }

# Split with globbing off (a `*` must not expand against the cwd). Quotes stay
# attached to their token, so `rg "cargo test"` never matches a command word.
set -f
# shellcheck disable=SC2206
TOKENS=($STRIPPED)
set +f
NTOK=${#TOKENS[@]}

# block <reason> - refuse the tool call and print the doctrine on stderr.
#
# Everything below the "BLOCKED" framing line is the SHARED no-verify doctrine
# (BLOCK-A), byte-identical to loom-hooks/_subagent-preamble.txt,
# loom/src/orchestrator/signals/cache/blocks.rs and tests_doctrine_blocks.rs.
# Reword one and you must reword all four:
# loom/src/orchestrator/signals/tests_doctrine.rs fails if they drift.
block() {
	loom_debug "DEBUG: BLOCKED - $1"
	if [[ "$ROLE" == main ]]; then
		loom_record_block subagent-verify-guard gate-once Bash "$COMMAND"
		cat >&2 <<EOF
[subagent-verify-guard/gate-once] ⛔ BLOCKED: the full suite runs only through loom check $STAGE_ID; re-check one criterion with loom check $STAGE_ID --criterion <n>; run your own work with a filtered test
EOF
		exit 2
	fi
	loom_record_block subagent-verify-guard subagent-verify Bash "$COMMAND"
	cat >&2 <<'EOF'
[subagent-verify-guard/subagent-verify] ⛔ BLOCKED: Subagent attempting project-wide verification.

VERIFICATION IS THE MAIN AGENT'S JOB - NOT YOURS:
- Compile the files YOU wrote as often as you need, and format them,
  with the project's own build and formatter commands scoped to those
  files. No linter and no formatter run over the whole project.
- Run the tests for the code YOU wrote, and its module, when you are
  confident your code is ready or your fix is correct. Never the whole
  test suite: the stage's gate runs it.
- Report instead: files changed, assumptions made, anything unresolved.
  The stage's gate compiles, tests, and lints; the MAIN AGENT fixes.
EOF
	exit 2
}

# Runner matchers (check_command and the helpers it calls). They read TOKENS,
# NTOK, ROLE and TEST_ASSIGNED and call block() when they run, so this only
# has to precede the walk loop below.
# shellcheck source=_verify_matchers.sh
# A missing helper lets the call through (exit 1 is non-blocking): this hook
# only raises the cost of a bare run, so its own breakage never stops work.
source "$(dirname "${BASH_SOURCE[0]}")/_verify_matchers.sh" || {
	echo "subagent-verify-guard.sh: _verify_matchers.sh is missing or unreadable beside this hook - test-runner checks are off for this call. Reinstall the hooks with \`loom install-assets\`." >&2
	exit 1
}

# Walk the token stream, inspecting only tokens in COMMAND POSITION (line start
# or just after a separator), so mentions inside arguments - `rg cargo doc/`,
# `echo make test` - are never treated as runs.
DQ='"'
SQ="'"
i=0
cmd_pos=1
TEST_ASSIGNED=0 # a TEST=/TESTOPTS= assignment precedes the current command word
in_quote=""
while [[ $i -lt $NTOK ]]; do
	TOK="${TOKENS[$i]}"

	# The splitter does not parse quotes, so a separator inside a quoted
	# argument (`rg "x && cargo build -v" src/`) would start a fake command, and
	# a quoted multi-line message would turn its prose into commands. Every
	# token inside an open quote is inert until it closes.
	if [[ -n "$in_quote" ]]; then
		case "$TOK" in
		*"$in_quote"*) in_quote="" ;;
		esac
		i=$((i + 1))
		continue
	fi

	# Does THIS token leave a quote open? (Only affects the tokens after it.)
	QCHARS=${TOK//[!$DQ]/}
	if [[ $((${#QCHARS} % 2)) -eq 1 ]]; then in_quote=$DQ; fi
	QCHARS=${TOK//[!$SQ]/}
	if [[ -z "$in_quote" && $((${#QCHARS} % 2)) -eq 1 ]]; then in_quote=$SQ; fi

	if [[ $cmd_pos -eq 1 ]]; then
		case "$TOK" in
		-u | --unset | -C | --chdir | -S | --split-string)
			# Prefix-command flag that takes a value (`env -u VAR cargo test`):
			# skip the value too, or it would be read as the command word
			i=$((i + 2))
			continue
			;;
		TEST=* | TESTOPTS=*)
			TEST_ASSIGNED=1
			i=$((i + 1))
			continue
			;;
		[A-Za-z_]*=* | env | sudo | time | nohup | command | exec | nice | xargs | npx | bunx | -*)
			# Env assignment or command prefix: the real command follows
			i=$((i + 1))
			continue
			;;
		esac
		check_command "$i"
	fi

	case "$TOK" in
	";" | "&&" | "||" | "|" | "&" | "(" | ")" | "{" | "}" | "!" | \
		"if" | "elif" | "while" | "until" | "then" | "do" | "else")
		cmd_pos=1
		TEST_ASSIGNED=0
		;;
	*) cmd_pos=0 ;;
	esac
	i=$((i + 1))
done

exit 0
