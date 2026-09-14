#!/usr/bin/env bash
# Trusted PostToolUse producer for a completed macOS direct Codex forward.

set -uo pipefail

HOOK_NAME=codex-forward-result.sh
source "$(dirname "$0")/_lifecycle.sh"
source "$(dirname "${BASH_SOURCE[0]}")/_codex_forward.sh"
command -v jq >/dev/null 2>&1 || exit 0

parse_forward_command() {
	parse_shell_words "$1" || return 1
	[[ ${#PARSED_WORDS[@]} -eq 12 ]] || return 1
	local wrapper=${PARSED_WORDS[0]}
	if [[ -n "${HOME:-}" ]]; then
		[[ "$wrapper" == "~/.claude/hooks/loom/codex-forward.sh" ||
			"$wrapper" == "$HOME/.claude/hooks/loom/codex-forward.sh" ]] || return 1
	else
		[[ "$wrapper" == "~/.claude/hooks/loom/codex-forward.sh" ]] || return 1
	fi
	[[ "${PARSED_WORDS[1]}" == task && -n "${PARSED_WORDS[2]}" &&
		"${PARSED_WORDS[3]}" == --model && "${PARSED_WORDS[5]}" == --effort &&
		"${PARSED_WORDS[7]}" == --write && "${PARSED_WORDS[8]}" == --unit-id &&
		"${PARSED_WORDS[10]}" == --invocation-id ]] || return 1
	case "${PARSED_WORDS[4]}" in gpt-6-astra | gpt-5.6-sol | gpt-5.6-terra | gpt-5.6-luna) ;; *) return 1 ;; esac
	case "${PARSED_WORDS[6]}" in low | medium | high | xhigh | max | ultra) ;; *) return 1 ;; esac
	loom_lifecycle_safe_id "${PARSED_WORDS[9]}" && [[ ${#PARSED_WORDS[9]} -le 64 ]] || return 1
	[[ "${PARSED_WORDS[11]}" =~ ^inv-[0-9a-f]{32}$ ]]
}

load_payload() {
	INPUT_JSON=$(loom_lifecycle_read_input)
	[[ -n "$INPUT_JSON" ]] || return 1
	PAYLOAD=$(printf '%s' "$INPUT_JSON" | jq -cer '
		select(type == "object" and .tool_name == "Bash" and
		 (.tool_input | type == "object") and (.tool_input.command | type == "string") and
		 (.tool_use_id | type == "string") and (.agent_id | type == "string") and
		 (.session_id | type == "string") and (.cwd | type == "string") and
		 (has("tool_result") or has("tool_response")))' 2>/dev/null) || return 1
	COMMAND=$(printf '%s' "$PAYLOAD" | jq -r '.tool_input.command')
	TOOL_USE_ID=$(printf '%s' "$PAYLOAD" | jq -r '.tool_use_id')
	FORWARDER_ID=$(printf '%s' "$PAYLOAD" | jq -r '.agent_id')
	PARENT_ID=$(printf '%s' "$PAYLOAD" | jq -r '.session_id')
	PAYLOAD_CWD=$(printf '%s' "$PAYLOAD" | jq -r '.cwd')
	parse_forward_command "$COMMAND"
}

load_authorization() {
	local ledger="$WORK/subagents/$STAGE/codex.jsonl" bytes auth_fields
	loom_lifecycle_plain_path "$ledger" file || return 1
	bytes=$(wc -c <"$ledger" 2>/dev/null) || return 1
	[[ "$bytes" =~ ^[0-9]+$ ]] && ((bytes > 0 && bytes <= 4194304)) || return 1
	AUTH_ROW=$(jq -sc --arg stage "$STAGE" --arg session "$SESSION" \
		--arg parent "$PARENT_ID" --arg forwarder "$FORWARDER_ID" \
		--arg tool "$TOOL_USE_ID" --arg unit "$UNIT" --arg invocation "$INVOCATION" \
		--arg model "$MODEL" --arg effort "$EFFORT" '
		if all(.[]; type == "object") then
		 [ .[] | select(
		  (keys | sort) == (["companion_path","companion_version","effort","forwarder_agent_id",
		   "invocation_id","model","parent_session_id","session_id","stage_id","state_root","tool_use_id",
		   "ts","unit_id","v","workspace_root"] | sort) and .v == 2 and
		  .stage_id == $stage and .session_id == $session and .parent_session_id == $parent and
		  .forwarder_agent_id == $forwarder and .tool_use_id == $tool and .unit_id == $unit and
		  .invocation_id == $invocation and .model == $model and .effort == $effort and
		  (.ts | type == "string") and (.workspace_root | type == "string") and
		  (.companion_version | type == "string") and (.companion_path | type == "string") and
		  (.state_root | type == "string")) ] | if length == 1 then .[0] else empty end
		else empty end' "$ledger" 2>/dev/null) || return 1
	[[ -n "$AUTH_ROW" ]] || return 1
	auth_fields=$(printf '%s' "$AUTH_ROW" | jq -er '[.ts,.workspace_root] | @tsv') || return 1
	IFS=$'\t' read -r AUTH_TS WORKSPACE <<<"$auth_fields"
	loom_lifecycle_epoch "$AUTH_TS" >/dev/null || return 1
	loom_lifecycle_plain_path "$WORKSPACE" dir || return 1
	local cwd
	cwd=$(cd "$PAYLOAD_CWD" 2>/dev/null && pwd -P) || return 1
	[[ "$cwd" == "$WORKSPACE" || "$cwd" == "$WORKSPACE"/* ]]
}

valid_persisted_output() {
	local path="$1" projects bytes before after newline_count captured sentinel=$'\034'
	[[ -n "${HOME:-}" && "$path" == /* ]] || return 1
	projects="$HOME/.claude/projects"
	case "$path" in ../* | */../* | */.. | ..) return 1 ;; esac
	[[ "$path" == "$projects"/* && "$path" == */tool-results/* && -f "$path" && ! -L "$path" ]] || return 1
	before=$(loom_lifecycle_stat_fingerprint "$path") || return 1
	bytes=$(wc -c <"$path" 2>/dev/null) || return 1
	[[ "$bytes" =~ ^[0-9]+$ ]] && ((bytes > 0 && bytes <= 262144)) || return 1
	newline_count=$(tail -c 1 "$path" 2>/dev/null | wc -l) || return 1
	[[ "$newline_count" =~ ^[[:space:]]*1[[:space:]]*$ ]] || return 1
	captured=$(command cat "$path" 2>/dev/null && printf '%s' "$sentinel") || return 1
	[[ "$captured" == *"$sentinel" ]] || return 1
	OUTPUT=${captured%"$sentinel"}
	[[ "$OUTPUT" == *$'\n' && "$OUTPUT" != *$'\n\n' ]] || return 1
	OUTPUT=${OUTPUT%$'\n'}
	[[ "$OUTPUT" != *"$sentinel"* ]] || return 1
	after=$(loom_lifecycle_stat_fingerprint "$path") || return 1
	[[ "$before" == "$after" ]]
}

load_output() {
	local inline captured persisted sentinel=$'\034'
	captured=$(printf '%s' "$PAYLOAD" | jq -j '
		[.tool_result.stdout, .tool_result.output, .tool_response.stdout, .tool_response.output]
		| map(select(type == "string")) | first // empty' && printf '%s' "$sentinel") || return 1
	[[ "$captured" == *"$sentinel" ]] || return 1
	inline=${captured%"$sentinel"}
	persisted=$(printf '%s' "$PAYLOAD" | jq -r \
		'.tool_response.persistedOutputPath // .tool_result.persistedOutputPath // empty') || return 1
	if [[ -z "$persisted" ]]; then
		persisted=$(printf '%s' "$inline" | sed -n 's/^.*Full output saved to: //p' | head -n 1)
	fi
	if [[ -n "$persisted" ]]; then
		valid_persisted_output "$persisted" || return 1
	else
		OUTPUT=$inline
		[[ "$OUTPUT" == *$'\n' && "$OUTPUT" != *$'\n\n' ]] || return 1
		OUTPUT=${OUTPUT%$'\n'}
		[[ "$OUTPUT" != *"$sentinel"* ]] || return 1
		[[ ${#OUTPUT} -le 262144 ]] || return 1
	fi
	[[ -n "$OUTPUT" ]]
}

parse_supervisor_result() {
	local fields
	fields=$(printf '%s' "$OUTPUT" | jq -Rser '
		split("\n") | map(select(startswith("LOOM-CODEX-DIRECT-RESULT "))) as $rows |
		select($rows | length == 1) |
		($rows[0] | ltrimstr("LOOM-CODEX-DIRECT-RESULT ") | fromjson?) as $r |
		select($r != null and ($r | type == "object") and
		 ($r | keys | sort) == (["exit_code","ownership_retained","state","terminal_at","thread_id","turn_id","v"] | sort) and
		 $r.v == 1 and ($r.state | IN("succeeded","failed","cancelled","unknown")) and
		 ($r.thread_id | type == "string") and ($r.turn_id | type == "string") and
		 ($r.terminal_at | type == "string") and ($r.exit_code | type == "number" and floor == .) and
		 ($r.ownership_retained | type == "boolean")) |
		[$r.state,$r.thread_id,$r.turn_id,$r.terminal_at,($r.exit_code|tostring),
		 ($r.ownership_retained|tostring)] | @tsv' 2>/dev/null) || return 1
	IFS=$'\t' read -r STATE THREAD_ID TURN_ID TERMINAL_AT RESULT_STATUS RETAINED <<<"$fields"
	loom_lifecycle_safe_id "$THREAD_ID" && loom_lifecycle_safe_id "$TURN_ID" || return 1
	[[ "$TERMINAL_AT" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{3}Z$ ]] || return 1
	case "$STATE:$RESULT_STATUS:$RETAINED" in
	succeeded:0:false | cancelled:124:false | unknown:125:true) ;;
	failed:*:false) ((RESULT_STATUS > 0 && RESULT_STATUS < 126)) || return 1 ;;
	*) return 1 ;;
	esac
}

validate_outer_result() {
	local outer
	outer=$(printf '%s' "$PAYLOAD" | jq -er '
		(.tool_result // .tool_response) as $r |
		select(($r | type) == "object" and ($r.is_error | type) == "boolean" and
		 ($r.exit_code | type) == "number" and ($r.exit_code | floor) == $r.exit_code) |
		[($r.exit_code|tostring),($r.is_error|tostring)] | @tsv') || return 1
	IFS=$'\t' read -r OUTER_STATUS OUTER_ERROR <<<"$outer"
	[[ "$OUTER_STATUS" == "$RESULT_STATUS" ]] || return 1
	if [[ "$RESULT_STATUS" == 0 ]]; then [[ "$OUTER_ERROR" == false ]]; else [[ "$OUTER_ERROR" == true ]]; fi
}

validate_wrapper_output() {
	local trailer protocol
	trailer=$(printf '%s\n' '--- LOOM-CODEX-EVIDENCE ---' "exit: $RESULT_STATUS" \
		'mode: direct (codex exec --sandbox danger-full-access; nested Seatbelt refused)' \
		"thread: $THREAD_ID" "unit: $UNIT" "invocation: $INVOCATION" "state: $STATE")
	trailer=${trailer%$'\n'}
	[[ "$OUTPUT" == *$'\n'"$trailer" ]] || return 1
	protocol=$(printf '%s' "$OUTPUT" | jq -Rser --arg thread "$THREAD_ID" --arg state "$STATE" \
		--argjson status "$RESULT_STATUS" '
		split("\n") as $lines |
		select($lines[0] | startswith("LOOM-FORWARD-START ")) |
		([$lines[] | select(startswith("LOOM-FORWARD-START "))] | length) as $starts |
		([$lines[] | select(startswith("LOOM-FORWARD-END "))] | length) as $ends |
		select($starts == 1 and $ends == 1) |
		($lines[0] | ltrimstr("LOOM-FORWARD-START ") | fromjson?) as $s |
		([$lines[] | select(startswith("LOOM-FORWARD-END "))][0] |
		 ltrimstr("LOOM-FORWARD-END ") | fromjson?) as $e |
		select(($s|keys|sort) == (["backend","thread_id","v"]|sort) and
		 ($e|keys|sort) == (["backend","exit_code","outcome","thread_id","v"]|sort) and
		 $s == {v:1,backend:"direct",thread_id:$thread} and
		 $e == {v:1,backend:"direct",thread_id:$thread,outcome:$state,exit_code:$status}) |
		"valid"' 2>/dev/null) || return 1
	[[ "$protocol" == valid ]]
}

event_id() {
	local state="$1" kind="$2" turn="$3" terminal="$4" outcome="$5" execution digest
	execution="direct:$THREAD_ID:$TOOL_USE_ID"
	if command -v sha256sum >/dev/null 2>&1; then
		digest=$(printf 'loom.lifecycle.codex.v1\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s' \
			codex_direct "$STAGE" "$SESSION" "$PARENT_ID" "$FORWARDER_ID" "$UNIT" "$INVOCATION" \
			"$WORKSPACE" "$execution" "$state" "$MODEL" "$EFFORT" "$kind" "$turn" "$terminal" "$outcome" |
			sha256sum 2>/dev/null) || return 1
	else
		digest=$(printf 'loom.lifecycle.codex.v1\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s' \
			codex_direct "$STAGE" "$SESSION" "$PARENT_ID" "$FORWARDER_ID" "$UNIT" "$INVOCATION" \
			"$WORKSPACE" "$execution" "$state" "$MODEL" "$EFFORT" "$kind" "$turn" "$terminal" "$outcome" |
			shasum -a 256 2>/dev/null) || return 1
	fi
	digest=${digest%% *}
	[[ "$digest" =~ ^[0-9a-f]{64}$ ]] && printf 'sha256:%s\n' "$digest"
}

make_record() {
	local event="$1" observed="$2" state="$3" kind="$4" turn="$5" terminal="$6" outcome="$7" detail="$8"
	jq -nc --arg event "$event" --arg stage "$STAGE" --arg session "$SESSION" \
		--arg parent "$PARENT_ID" --arg forwarder "$FORWARDER_ID" --arg unit "$UNIT" \
		--arg invocation "$INVOCATION" --arg workspace "$WORKSPACE" --arg thread "$THREAD_ID" \
		--arg tool "$TOOL_USE_ID" --arg observed "$observed" --arg state "$state" --arg model "$MODEL" \
		--arg effort "$EFFORT" --arg kind "$kind" --arg turn "$turn" --arg terminal "$terminal" \
		--arg outcome "$outcome" --arg detail "$detail" '
		{version:1,event_id:$event,producer:"codex_direct",
		 identity:{kind:"codex",stage_id:$stage,loom_session_id:$session,parent_session_id:$parent,
		  forwarder_agent_id:$forwarder,unit_id:$unit,invocation_id:$invocation,workspace_root:$workspace,
		  execution:{mode:"direct",thread_id:$thread,tool_use_id:$tool}},
		 observed_at:$observed,state:$state,
		 evidence:{evidence_kind:$kind,requested_model:$model,requested_effort:$effort,
		  invocation_id:$invocation,job_id:null,thread_id:$thread,
		  turn_id:(if $turn == "" then null else $turn end),tool_use_id:$tool,
		  terminal_at:(if $terminal == "" then null else $terminal end),outcome:$outcome,
		  detail:(if $detail == "" then null else $detail end)}}'
}

append_records() {
	local lifecycle_state outcome detail= terminal_hash auth_event observation_event auth observation
	case "$STATE" in
	succeeded) lifecycle_state=completed; outcome=succeeded ;;
	failed) lifecycle_state=failed; outcome=failed ;;
	cancelled) lifecycle_state=cancelled; outcome=cancelled ;;
	unknown) lifecycle_state=unknown; outcome=unknown; detail='ownership retained' ;;
	*) return 1 ;;
	esac
	terminal_hash=${TERMINAL_AT%Z}+00:00
	auth_event=$(event_id running authorization '' '' running) || return 1
	observation_event=$(event_id "$lifecycle_state" observation "$TURN_ID" "$terminal_hash" "$outcome") || return 1
	auth=$(make_record "$auth_event" "$AUTH_TS" running authorization '' '' running '') || return 1
	observation=$(make_record "$observation_event" "$TERMINAL_AT" "$lifecycle_state" observation \
		"$TURN_ID" "$TERMINAL_AT" "$outcome" "$detail") || return 1
	loom_lifecycle_append "$WORK" "$STAGE" "$SESSION" "$auth"$'\n'"$observation" "$HOOK_NAME"
}

main() {
	load_payload || return 0
	STAGE=${LOOM_STAGE_ID:-}; SESSION=${LOOM_SESSION_ID:-}
	loom_lifecycle_safe_id "$STAGE" && loom_lifecycle_safe_id "$SESSION" &&
		loom_lifecycle_safe_id "$TOOL_USE_ID" && loom_lifecycle_safe_id "$FORWARDER_ID" &&
		loom_lifecycle_safe_id "$PARENT_ID" || return 0
	MODEL=${PARSED_WORDS[4]}; EFFORT=${PARSED_WORDS[6]}
	UNIT=${PARSED_WORDS[9]}; INVOCATION=${PARSED_WORDS[11]}
	WORK=$(loom_lifecycle_resolve_work_root "${LOOM_WORK_DIR:-}") || return 0
	load_authorization || return 0
	load_output || return 0
	[[ "$OUTPUT" != *$'\nmode: companion\n'* ]] || return 0
	parse_supervisor_result || return 0
	validate_outer_result || return 0
	validate_wrapper_output || return 0
	append_records >/dev/null 2>&1 || true
	return 0
}

main
exit 0
