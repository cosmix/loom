#!/usr/bin/env bash
# Shared hook helpers; Bash 3.2 compatible.
if [[ "${_LOOM_SPAWN_POLICY_LOADED:-}" == 1 ]]; then return 0; fi
_LOOM_SPAWN_POLICY_LOADED=1

# loom_reviewer_model_override - spawn-guard's loom-code-reviewer model pin.
# loom-code-reviewer runs the operator-configured model and effort
# ([models] code_reviewer_model / code_reviewer_effort), even over an explicit
# caller MODEL_REQ: callers downgrade reviewers. On success sets the caller's
# LOOM_SPAWN_MODEL, LOOM_SPAWN_EFFORT, LOOM_SPAWN_MODEL_SOURCE, LOOM_SPAWN_NEEDS_REWRITE, clears LOOM_SPAWN_WARN_ESCALATION and may
# set LOOM_SPAWN_WARN_REVIEWER. A missing binary changes nothing; a failing or malformed
# `loom hook reviewer-model` keeps the frontmatter model and sets
# WARN_REVIEWER_FAIL, so a broken config.toml is visible to the operator.
loom_reviewer_model_override() {
	local MODEL_REQ="$1"
	local bin="${LOOM_BIN:-loom}" line errfile err="" reason=""
	command -v "$bin" &>/dev/null || return 0
	errfile=$(mktemp "${TMPDIR:-/tmp}/loom-reviewer-model.XXXXXX" 2>/dev/null) || errfile=/dev/null
	if ! line=$(loom_run_bounded 5 "$bin" hook reviewer-model 2>"$errfile"); then
		loom_debug "spawn-guard: reviewer-model timeout or nonzero exit"
		# First stderr line, control characters stripped, capped at 200 chars.
		err=$(head -n 1 "$errfile" 2>/dev/null | LC_ALL=C tr -d '\000-\037\177' | cut -c1-200) || err=""
		reason="${err:+ (${err})}"
	elif [[ "$line" =~ ^model=([a-z0-9.-]+)\ effort=([a-z]+)$ ]]; then
		[[ "$errfile" == /dev/null ]] || rm -f "$errfile"
		if [[ -n "$MODEL_REQ" && "$MODEL_REQ" != "${BASH_REMATCH[1]}" ]]; then
			LOOM_SPAWN_WARN_REVIEWER="loom-code-reviewer runs the configured model ${BASH_REMATCH[1]}/${BASH_REMATCH[2]} (models.code_reviewer_model); the requested '${MODEL_REQ}' was replaced"
		fi
		LOOM_SPAWN_MODEL="${BASH_REMATCH[1]}"
		LOOM_SPAWN_EFFORT="${BASH_REMATCH[2]}"
		LOOM_SPAWN_MODEL_SOURCE="config"
		LOOM_SPAWN_NEEDS_REWRITE=1
		LOOM_SPAWN_WARN_ESCALATION=""
		return 0
	else
		loom_debug "spawn-guard: reviewer-model malformed output"
	fi
	[[ "$errfile" == /dev/null ]] || rm -f "$errfile"
	LOOM_SPAWN_WARN_REVIEWER="loom hook reviewer-model failed${reason}; loom-code-reviewer runs its frontmatter model. Check models.code_reviewer_model in ~/.loom/config.toml / .loom/work/config.toml"
}

# _loom_brief_json ENVELOPE MARKER - validates a `loom hook worker-brief` or
# `loom hook reviewer-brief` envelope (MARKER loom-worker-brief or
# loom-reviewer-brief) and prints the brief as a JSON string, or nothing. Valid:
# one line, an object whose keys are exactly `brief` and `nonce`, a 32-hex
# nonce, a non-empty string brief holding exactly one line equal to
# `<!-- MARKER nonce=NONCE -->`. With a third argument `first` the marker line must
# also be the brief's first line and no other line may start with `<!-- loom-`.
_loom_brief_json() {
	local envelope="$1" marker="$2" strict="${3:-}"
	[[ "$envelope" != *$'\n'* ]] || return 0
	printf '%s' "$envelope" | jq -ec --arg marker "$marker" --arg strict "$strict" '
		select(type == "object" and (keys | sort) == ["brief", "nonce"])
		| .nonce as $nonce
		| select(($nonce | type) == "string" and ($nonce | test("^[0-9a-f]{32}$")))
		| select((.brief | type) == "string" and (.brief | length) > 0)
		| select(([.brief | split("\n")[] | select(. == ("<!-- " + $marker + " nonce=" + $nonce + " -->"))] | length) == 1)
		| (.brief | split("\n")) as $lines
		| select($strict != "first" or (
			$lines[0] == ("<!-- " + $marker + " nonce=" + $nonce + " -->")
			and ([$lines[1:][] | select(startswith("<!-- loom-"))] | length) == 0))
		| .brief' 2>/dev/null || true
}

# loom_worker_brief - spawn-guard's scoped worker brief. Sets LOOM_SPAWN_WORKER_BRIEF_JSON
# (empty when none) from INPUT_JSON, TOOL_INPUT, GATE_PASSED, AGENT_TYPE_REQ.
# A missing command, timeout, bad envelope or empty selection changes nothing.
# Codex-bound types get none: Codex units read loom's Codex UserPromptSubmit
# brief (fs/permissions/codex_hooks.rs). The prompt is read from TOOL_INPUT so
# its trailing newlines survive the merge.
loom_worker_brief() {
	local INPUT_JSON="$1" TOOL_INPUT="$2" GATE_PASSED="$3" AGENT_TYPE_REQ="$4" envelope
	LOOM_SPAWN_WORKER_BRIEF_JSON=""
	if [[ $GATE_PASSED -eq 1 && "$AGENT_TYPE_REQ" != loom-codex-forwarder && "$AGENT_TYPE_REQ" != codex:* ]] && command -v "${LOOM_BIN:-loom}" &>/dev/null &&
		printf '%s' "$TOOL_INPUT" | jq -e 'type == "object" and (.prompt | type == "string")' >/dev/null 2>&1; then
		if ! envelope=$(printf '%s' "$INPUT_JSON" | LOOM_HOOK_CONTEXT=1 loom_run_bounded 5 "${LOOM_BIN:-loom}" hook worker-brief 2>/dev/null) || [[ -z "$envelope" ]]; then
			loom_debug "spawn-guard: worker-brief timeout, nonzero exit or empty output"
		else
			LOOM_SPAWN_WORKER_BRIEF_JSON=$(_loom_brief_json "$envelope" loom-worker-brief)
			[[ -n "$LOOM_SPAWN_WORKER_BRIEF_JSON" ]] || loom_debug "spawn-guard: worker-brief invalid envelope"
		fi
	fi
}

# loom_reviewer_brief TOOL_INPUT GATE_PASSED AGENT_TYPE_REQ - the stage's test-integrity
# brief for a loom-code-reviewer. Sets LOOM_SPAWN_REVIEWER_BRIEF_JSON (empty when none).
# Runs only inside a live stage session (GATE_PASSED 1) for exactly loom-code-reviewer
# with a string prompt. `loom hook reviewer-brief` reads no stdin, so it gets /dev/null:
# piping a payload into a command that never reads it dies of SIGPIPE under pipefail.
# A missing command, timeout, bad envelope or empty output changes nothing.
loom_reviewer_brief() {
	local TOOL_INPUT="$1" GATE_PASSED="$2" AGENT_TYPE_REQ="$3" envelope
	LOOM_SPAWN_REVIEWER_BRIEF_JSON=""
	[[ $GATE_PASSED -eq 1 && "$AGENT_TYPE_REQ" == loom-code-reviewer ]] || return 0
	command -v "${LOOM_BIN:-loom}" &>/dev/null || return 0
	printf '%s' "$TOOL_INPUT" | jq -e 'type == "object" and (.prompt | type == "string")' >/dev/null 2>&1 || return 0
	if ! envelope=$(loom_run_bounded 5 "${LOOM_BIN:-loom}" hook reviewer-brief </dev/null 2>/dev/null) || [[ -z "$envelope" ]]; then
		loom_debug "spawn-guard: reviewer-brief timeout, nonzero exit or empty output"
		return 0
	fi
	LOOM_SPAWN_REVIEWER_BRIEF_JSON=$(_loom_brief_json "$envelope" loom-reviewer-brief first)
	[[ -n "$LOOM_SPAWN_REVIEWER_BRIEF_JSON" ]] || loom_debug "spawn-guard: reviewer-brief invalid envelope"
}

# loom_append_brief BRIEF_JSON - appends a validated brief (a JSON string, possibly
# empty) to the caller's UPDATED_INPUT prompt after a blank line and sets the caller's
# PROMPT_REWRITTEN. Empty input or a jq failure changes nothing.
loom_append_brief() {
	local updated
	[[ -n "$1" ]] || return 0
	if updated=$(printf '%s' "$UPDATED_INPUT" | jq -c --argjson brief "$1" \
		'. + {prompt: (.prompt + "\n\n" + $brief)}' 2>/dev/null) && [[ -n "$updated" ]]; then
		UPDATED_INPUT="$updated"
		PROMPT_REWRITTEN=1
	else
		loom_debug "spawn-guard: brief append failed"
	fi
}
