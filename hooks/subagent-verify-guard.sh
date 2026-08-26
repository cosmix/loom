#!/usr/bin/env bash
# subagent-verify-guard.sh - PreToolUse hook blocking full-suite runs by SUBAGENTS
#
# Verification is the MAIN AGENT's job: a subagent running the full build/test/
# lint/typecheck suite burns wall-clock and tokens and surfaces failures it was
# never asked to own. Project-wide runners are blocked at the tool boundary,
# narrowly-scoped runs (a filter, one test target, a path) allowed.
#
# The MAIN AGENT IS NEVER AFFECTED: the hook acts only when `loom_is_subagent`
# (hooks/_common.sh) says so. That check gates on a LIVE loom session FIRST,
# ALWAYS - LOOM_MAIN_AGENT_PID must be set and a live process-tree ancestor -
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
# between the caller and LOOM_MAIN_AGENT_PID. integration-verify stages are
# carved out below (their review subagents are supposed to run the full
# suite). There is deliberately NO escape-hatch env var - an opt-out would
# defeat the whole purpose (commit-filter.sh already treats unsetting the
# detection gate as evasion).
#
# SECURITY NOTE (best-effort, defense-in-depth): matching is token-based, not a
# shell parser, so a runner hidden in a quoted string (`bash -c "cargo test"`)
# or built by substitution is not seen. Rules are CONSERVATIVE - an unrecognised
# command is allowed, since a false block strands a subagent mid-task; the
# durable guarantee is doctrinal, and this hook only raises the cost.
#
# Input: JSON from stdin - {"tool_name": "Bash", "tool_input": {"command": ...}}
# Exit codes: 0 = allow, 2 = block with guidance on stderr

set -euo pipefail

source "$(dirname "$0")/_common.sh"

# Read stdin under gtimeout (macOS+coreutils), timeout (Linux), or bare cat
if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
	INPUT_JSON=$(cat 2>/dev/null || true)
fi

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

# === SUBAGENT GATE === everything below is subagent-only; a main agent exits here
loom_is_subagent "$INPUT_JSON" || exit 0

# === INTEGRATION-VERIFY CARVE-OUT ===
# WHY LOOM_STAGE_ID IS SAFE HERE AND ONLY HERE: LOOM_STAGE_ID leaks into plain
# Claude Code sessions (knowledge mistakes.md, "Worktree-Isolation Hooks Gated
# on LOOM_STAGE_ID"), so it must never be an activation gate. It is not one
# here: the subagent gate above ALWAYS proves LOOM_MAIN_AGENT_PID is a LIVE
# ancestor FIRST - unconditionally, before it ever consults the payload to
# classify main-vs-subagent - so we are demonstrably inside a live loom
# session by the time this line runs, and this is a carve-out that only ever
# RELAXES the hook. Do not "fix" this into a gate.
#
# AMBIGUITY IS FAIL-SAFE: stage files carry a depth prefix (02-<id>.md), so the
# glob can match more than once. Taking the first would let a planted
# `00-<id>.md` claiming integration-verify beat the real file, and a hook with
# no escape hatch must not ship a cheap one. Preferring the match whose `id:`
# agrees is no defence either - whoever writes the decoy writes its `id:`. So:
# exactly ONE match may be consulted; duplicates, a missing file, or one not
# declaring integration-verify all fall through to the checks below.
STAGE_ID="${LOOM_STAGE_ID:-}"
WORK_DIR="${LOOM_WORK_DIR:-}"
if [[ -n "$STAGE_ID" && -n "$WORK_DIR" ]]; then
	STAGE_MATCHES=()
	for candidate in "${WORK_DIR}/stages/"*"-${STAGE_ID}.md"; do
		if [[ -f "$candidate" ]]; then STAGE_MATCHES+=("$candidate"); fi
	done

	STAGE_FILE=""
	if [[ ${#STAGE_MATCHES[@]} -eq 1 ]]; then
		STAGE_FILE="${STAGE_MATCHES[0]}"
	elif [[ ${#STAGE_MATCHES[@]} -eq 0 ]]; then
		STAGE_FILE="${WORK_DIR}/stages/${STAGE_ID}.md"
	else
		loom_debug "DEBUG: ${#STAGE_MATCHES[@]} stage files match $STAGE_ID - no carve-out"
	fi

	if [[ -n "$STAGE_FILE" && -r "$STAGE_FILE" ]] &&
		grep -qE '^stage_type:[[:space:]]*integration-verify' "$STAGE_FILE" 2>/dev/null; then
		loom_debug "DEBUG: integration-verify stage $STAGE_ID - full-suite runs allowed"
		exit 0
	fi
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
# Everything below the "BLOCKED" framing line is the SHARED no-verify doctrine,
# byte-identical to the copies in loom/src/orchestrator/signals/cache.rs and
# CLAUDE.md.template - a subagent meets the same words in its signal, in
# CLAUDE.md, and here. Reword one and you must reword all three:
# loom/src/orchestrator/signals/tests_doctrine.rs fails if they drift.
block() {
	loom_debug "DEBUG: BLOCKED - $1"
	cat >&2 <<'EOF'
⛔ BLOCKED: Subagent attempting project-wide verification.

VERIFICATION IS THE MAIN AGENT'S JOB - NOT YOURS:
- Do NOT verify your work. No full build, no full test suite, no linter, no
  formatter, no type-checker, and never a repeated or looping check.
- AT MOST ONE narrowly-scoped check over the files YOU wrote (e.g.
  `cargo test <your_module>::`), run ONCE. Skip it if you are unsure.
- Report instead: files changed, assumptions made, anything unresolved.
  The MAIN AGENT compiles, tests, lints, and fixes.
EOF
	exit 2
}

# is_redirect_op <token> - a BARE redirection operator (`>`, `>>`, `2>`, `<`)
# whose target is the NEXT token. `2>&1` names its own target, so it is not one.
is_redirect_op() {
	[[ -n "$1" ]] || return 1
	case "$1" in
	*[!0-9\<\>\&]*) return 1 ;; # not made purely of redirection characters
	*\&[0-9]*) return 1 ;;      # `2>&1` - the target is part of the token
	*[\<\>]*) return 0 ;;
	esac
	return 1
}

# tok_kind <token> - sets TOK_KIND to sep | redirop | redir | word.
#
# A redirection is NEVER a positional argument. `cargo test 2>&1 | tail -50` is
# a FULL-SUITE run, and CLAUDE.md rule 14 (pipe tests through tail) makes it the
# likeliest invocation here; reading `2>&1` as a filter would wave it through.
TOK_KIND=""
tok_kind() {
	case "$1" in
	";" | "&&" | "||" | "|" | "&" | "(" | ")" | "{" | "}") TOK_KIND=sep ;;
	*[\<\>]*)
		if is_redirect_op "$1"; then TOK_KIND=redirop; else TOK_KIND=redir; fi
		;;
	*) TOK_KIND=word ;;
	esac
	return 0
}

# scan_args <start-index> - classify a command's argument list. Echoes:
#   all    - an explicit whole-project flag (--all/--workspace/--all-targets/--doc)
#   scoped - a positional argument (filter/path), or a scoping flag, was given
#   bare   - only flags, or nothing: the project-wide default run
#
# Stops at the next separator, so `cargo test -p loom && cargo build` is judged
# one command at a time. Flags taking a separate value (--manifest-path <path>)
# consume it, so it is not read as a positional.
scan_args() {
	local i="$1" skip=0 tok
	while [[ $i -lt $NTOK ]]; do
		tok="${TOKENS[$i]}"
		i=$((i + 1))
		tok_kind "$tok"
		if [[ "$TOK_KIND" == sep ]]; then break; fi
		if [[ "$TOK_KIND" == redirop ]]; then skip=1; continue; fi
		if [[ "$TOK_KIND" == redir ]]; then continue; fi
		if [[ $skip -eq 1 ]]; then skip=0; continue; fi
		case "$tok" in
		--all | --workspace | --all-targets | --doc) echo "all"; return 0 ;;
		--test | --test=* | --bin | --bin=* | --example | --example=* | --bench | --bench=* | \
			-p | --package | --package=* | -k | -m) echo "scoped"; return 0 ;;
		--manifest-path | --target-dir | --features | -F | --target | --profile | \
			--color | --message-format | --config | -j | --jobs) skip=1 ;;
		--) ;;
		-*) ;;
		*) echo "scoped"; return 0 ;;
		esac
	done
	echo "bare"
	return 0
}

# first_positional <idx> <value-flags> - first bare arg before the next separator.
first_positional() {
	local i="$1" vflags=" $2 " tok
	while [[ $i -lt $NTOK ]]; do
		tok="${TOKENS[$i]}"
		i=$((i + 1))
		tok_kind "$tok"
		if [[ "$TOK_KIND" == sep ]]; then break; fi
		if [[ "$TOK_KIND" == redirop ]]; then i=$((i + 1)); continue; fi
		if [[ "$TOK_KIND" == redir ]]; then continue; fi
		case "$vflags" in *" $tok "*) i=$((i + 1)); continue ;; esac
		case "$tok" in -*) continue ;; esac
		printf '%s' "$tok"
		return 0
	done
	return 0
}

# block_on_word <idx> <words> <label> - block if one of <words> is a bare arg.
block_on_word() {
	local i="$1" words=" $2 " label="$3" tok
	while [[ $i -lt $NTOK ]]; do
		tok="${TOKENS[$i]}"
		i=$((i + 1))
		tok_kind "$tok"
		if [[ "$TOK_KIND" == sep ]]; then break; fi
		# `make docs > test.log`: a redirection target is a filename, not a goal
		if [[ "$TOK_KIND" == redirop ]]; then i=$((i + 1)); continue; fi
		if [[ "$TOK_KIND" == redir ]]; then continue; fi
		case "$tok" in -j | -C | -f | --directory) i=$((i + 1)); continue ;; esac
		case "$words" in *" $tok "*) block "$label $tok" ;; esac
	done
	return 0
}

# cargo: build/clippy/fmt/check are project-wide in any form. `cargo test` and
# `cargo nextest run` are blocked unless they name a scope (filter, --test, -p).
check_cargo() {
	local i="$1" sub=""
	# Skip a toolchain override (`cargo +nightly test`) and leading flags
	while [[ $i -lt $NTOK ]]; do
		sub="${TOKENS[$i]}"
		case "$sub" in
		+* | -*) i=$((i + 1)) ;;
		*) break ;;
		esac
	done
	[[ $i -lt $NTOK ]] || return 0

	sub="${TOKENS[$i]}"
	case "$sub" in
	build | clippy | fmt | check) block "cargo $sub" ;;
	test) block_unless_scoped $((i + 1)) "cargo test" ;;
	nextest)
		if [[ "${TOKENS[$((i + 1))]:-}" == "run" ]]; then
			block_unless_scoped $((i + 2)) "cargo nextest run"
		fi
		;;
	esac
	return 0
}

# block_unless_scoped <idx> <label> - block unless the args name a scope.
block_unless_scoped() {
	local verdict
	verdict=$(scan_args "$1")
	if [[ "$verdict" != "scoped" ]]; then block "$2 ($verdict)"; fi
	return 0
}

# tsc: -p/-b and a bare `tsc` typecheck everything; only a file list is scoped.
check_tsc() {
	block_on_word "$1" "-p --project -b --build -w --watch" "tsc whole-project"
	if [[ -z "$(first_positional "$1" "--outDir --outFile --target -t --module --lib --rootDir")" ]]; then
		block "tsc (no file argument)"
	fi
	return 0
}

check_eslint() {
	local pos
	pos=$(first_positional "$1" "--ext --config -c --format -f --rulesdir --resolve-plugins-relative-to")
	case "$pos" in
	"" | "." | "./" | "*" | "**" | "**/*") block "eslint over the whole project" ;;
	esac
	return 0
}

# go test/build/vet ./... walks the whole module; ./pkg/... is scoped.
check_go() {
	local sub="${TOKENS[$1]:-}"
	case "$sub" in
	test | build | vet) block_on_word $(($1 + 1)) "./... ... all" "go $sub" ;;
	esac
	return 0
}

check_node_pm() {
	local i="$1" pm="$2" tok
	tok="${TOKENS[$i]:-}"
	case "$tok" in
	run | run-script | exec)
		i=$((i + 1))
		tok="${TOKENS[$i]:-}"
		;;
	esac
	case "$tok" in
	test | build | lint | typecheck) block_unless_scoped $((i + 1)) "$pm $tok" ;;
	esac
	return 0
}

check_make() {
	block_on_word "$1" "test build check lint" "make"
	return 0
}

check_command() {
	local idx="$1"
	local cmd="${TOKENS[$idx]}"
	# `##*/` so an absolute path to the runner (/usr/bin/pytest) still matches
	local base="${cmd##*/}"
	case "$base" in
	cargo) check_cargo $((idx + 1)) ;;
	pytest) block_unless_scoped $((idx + 1)) "pytest" ;;
	tsc) check_tsc $((idx + 1)) ;;
	eslint) check_eslint $((idx + 1)) ;;
	go) check_go $((idx + 1)) ;;
	npm | bun | pnpm | yarn) check_node_pm $((idx + 1)) "$base" ;;
	make) check_make $((idx + 1)) ;;
	esac
	return 0
}

# Walk the token stream, inspecting only tokens in COMMAND POSITION (line start
# or just after a separator), so mentions inside arguments - `rg cargo doc/`,
# `echo make test` - are never treated as runs.
DQ='"'
SQ="'"
i=0
cmd_pos=1
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
		;;
	*) cmd_pos=0 ;;
	esac
	i=$((i + 1))
done

exit 0
