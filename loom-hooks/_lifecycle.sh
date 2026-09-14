#!/usr/bin/env bash
# _lifecycle.sh - Shared Claude worker lifecycle evidence helpers.

if [[ "${_LOOM_LIFECYCLE_LOADED:-}" == "1" ]]; then
	return 0
fi
_LOOM_LIFECYCLE_LOADED=1

source "$(dirname "${BASH_SOURCE[0]}")/_common.sh"

loom_lifecycle_safe_id() {
	local value="$1"
	[[ "$value" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$ ]]
}

loom_lifecycle_resolve_work_root() {
	local root="$1" resolved=""
	[[ -n "$root" && -d "$root" ]] || return 1
	resolved=$(cd -- "$root" 2>/dev/null && pwd -P) || return 1
	loom_lifecycle_plain_path "$resolved" dir || return 1
	printf '%s\n' "$resolved"
}

loom_lifecycle_plain_path() {
	local path="$1" kind="$2" current="" part="" remaining=""
	[[ "$path" == /* && "$path" != */./* && "$path" != */../* &&
		"$path" != */. && "$path" != */.. && "$path" != *//* ]] || return 1
	remaining=${path#/}
	while [[ -n "$remaining" ]]; do
		part=${remaining%%/*}
		[[ -n "$part" ]] || return 1
		current="${current}/${part}"
		[[ ! -L "$current" ]] || return 1
		if [[ "$remaining" == */* ]]; then remaining=${remaining#*/}; else remaining=""; fi
	done
	if [[ "$kind" == "file" ]]; then
		[[ -f "$path" ]]
	else
		[[ "$kind" == "dir" && -d "$path" ]]
	fi
}

loom_lifecycle_find_stage() {
	local work="$1" stage="$2" exact="$work/stages/$stage.md"
	local candidate="" basename="" prefix="" unsafe=0
	local matches=()
	loom_lifecycle_plain_path "$work/stages" dir || return 1
	if [[ -e "$exact" || -L "$exact" ]]; then
		if [[ -f "$exact" && ! -L "$exact" ]]; then matches+=("$exact"); else unsafe=1; fi
	fi
	for candidate in "$work"/stages/[0-9]*-"$stage".md; do
		[[ -e "$candidate" || -L "$candidate" ]] || continue
		basename=${candidate##*/}
		prefix=${basename%-${stage}.md}
		[[ -n "$prefix" && "$prefix" != *[!0-9]* ]] || continue
		if [[ -f "$candidate" && ! -L "$candidate" ]]; then
			matches+=("$candidate")
		else
			unsafe=1
		fi
	done
	[[ $unsafe -eq 0 && ${#matches[@]} -eq 1 ]] || return 1
	loom_lifecycle_plain_path "${matches[0]}" file || return 1
	printf '%s\n' "${matches[0]}"
}

loom_lifecycle_trim() {
	local value="$1"
	value="${value#"${value%%[![:space:]]*}"}"
	value="${value%"${value##*[![:space:]]}"}"
	printf '%s' "$value"
}

loom_lifecycle_stage_binding() {
	local work="$1" stage="$2" session="$3" file="" line="" value=""
	local line_number=0 id_seen=0 session_seen=0 closed=0 file_stage="" file_session=""
	file=$(loom_lifecycle_find_stage "$work" "$stage") || return 1
	while IFS= read -r line || [[ -n "$line" ]]; do
		line_number=$((line_number + 1))
		if [[ $line_number -eq 1 ]]; then
			[[ "$line" == "---" ]] || return 1
			continue
		fi
		if [[ "$line" == "---" ]]; then closed=1; break; fi
		case "$line" in
		id:*)
			id_seen=$((id_seen + 1)); value=$(loom_lifecycle_trim "${line#id:}")
			file_stage="$value"
			;;
		session:*)
			session_seen=$((session_seen + 1)); value=$(loom_lifecycle_trim "${line#session:}")
			file_session="$value"
			;;
		esac
	done <"$file"
	[[ $closed -eq 1 && $id_seen -eq 1 && $session_seen -eq 1 ]] || return 1
	loom_lifecycle_safe_id "$file_stage" && loom_lifecycle_safe_id "$file_session" || return 1
	[[ "$file_session" != "null" && "$file_session" != "~" ]] || return 1
	[[ "$file_stage" == "$stage" && "$file_session" == "$session" ]]
}

loom_lifecycle_epoch() {
	local value="$1" epoch=""
	[[ "$value" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?(Z|[+-][0-9]{2}:[0-9]{2})$ ]] || return 1
	epoch=$(date -u -d "$value" +%s 2>/dev/null || true)
	if [[ ! "$epoch" =~ ^[0-9]+$ ]]; then
		epoch=$(date -j -u -f '%Y-%m-%dT%H:%M:%S.000Z' "$value" +%s 2>/dev/null || true)
	fi
	[[ "$epoch" =~ ^[0-9]+$ ]] && printf '%s\n' "$epoch"
}

loom_lifecycle_log_jq_failure() {
	local hook="$1" operation="$2" status="$3"
	loom_debug "$hook: defect - jq failed while $operation (exit $status)"
}

loom_lifecycle_jq_value() {
	local input="$1" program="$2" hook="$3" operation="$4" output="" status=0
	output=$(printf '%s' "$input" | jq -r "$program" 2>/dev/null)
	status=$?
	if ((status != 0)); then
		loom_lifecycle_log_jq_failure "$hook" "$operation" "$status"
		return 2
	fi
	printf '%s' "$output"
}

loom_lifecycle_start_candidate() {
	local row="$1" stage="$2" parent="$3" loom_session="$4" agent="$5" hook="$6"
	local output="" status=0
	output=$(printf '%s' "$row" | jq -c --arg stage "$stage" --arg parent "$parent" \
		--arg loom "$loom_session" --arg agent "$agent" '
		select(
			.stage_id == $stage and .parent_session_id == $parent and
			.loom_session_id == $loom and .agent_id == $agent and
			((.agent_type? | type) == "string") and
			(.agent_type | (test("\\S") and (test("[\\r\\n]") | not)))
		) |
		[.agent_type, (if ((.ts? | type) == "string" and
			(.ts | (test("\\S") and (test("[\\r\\n]") | not)))) then .ts else null end)]' \
		2>/dev/null)
	status=$?
	if ((status != 0)); then
		loom_lifecycle_log_jq_failure "$hook" "resolving a SubagentStart row" "$status"
		return 2
	fi
	printf '%s' "$output"
}

loom_lifecycle_resolve_start() {
	local work="$1" stage="$2" parent="$3" loom_session="$4" agent="$5"
	local expected_type="$6" observed="$7" hook="$8" root="$work/subagents"
	local dir="" ledger="" row="" candidate="" row_type="" row_ts="" bytes=""
	local found=0 known_type="" known_ts="" start_epoch="" observed_epoch="" status=0
	loom_lifecycle_plain_path "$root" dir || return 1
	for dir in "$root"/*; do
		[[ -e "$dir" || -L "$dir" ]] || continue
		[[ ! -L "$dir" ]] || return 1
		[[ -d "$dir" ]] || continue
		ledger="$dir/starts.jsonl"
		[[ -e "$ledger" || -L "$ledger" ]] || continue
		loom_lifecycle_plain_path "$ledger" file || return 1
		bytes=$(wc -c <"$ledger" 2>/dev/null) || return 1
		[[ "$bytes" =~ ^[0-9]+$ ]] && ((bytes <= 4194304)) || return 1
		while IFS= read -r row; do
			[[ -n "${row//[[:space:]]/}" ]] || continue
			candidate=$(loom_lifecycle_start_candidate "$row" "$stage" "$parent" \
				"$loom_session" "$agent" "$hook"); status=$?
			((status == 0)) || return "$status"
			[[ -n "$candidate" ]] || continue
			[[ "${dir##*/}" == "$stage" ]] || return 1
			row_type=$(loom_lifecycle_jq_value "$candidate" '.[0]' "$hook" \
				"reading a SubagentStart agent type"); status=$?
			((status == 0)) || return "$status"
			row_ts=$(loom_lifecycle_jq_value "$candidate" '.[1] // empty' "$hook" \
				"reading an optional SubagentStart timestamp"); status=$?
			((status == 0)) || return "$status"
			if [[ $found -eq 1 && "$known_type" != "$row_type" ]]; then return 1; fi
			if [[ -n "$known_ts" && -n "$row_ts" && "$known_ts" != "$row_ts" ]]; then return 1; fi
			known_type="$row_type"; [[ -n "$known_ts" ]] || known_ts="$row_ts"; found=1
		done <"$ledger"
	done
	[[ $found -eq 1 && "$known_type" == "$expected_type" && -n "$known_ts" ]] || return 1
	start_epoch=$(loom_lifecycle_epoch "$known_ts") || return 1
	observed_epoch=$(loom_lifecycle_epoch "$observed") || return 1
	((start_epoch < observed_epoch)) && return 0
	((start_epoch == observed_epoch)) || return 1
	if [[ "$known_ts" =~ \.([0-9]+)(Z|[+-]) ]]; then
		[[ "${BASH_REMATCH[1]}" != *[!0]* ]]
	fi
}

loom_lifecycle_stat_fingerprint() {
	local path="$1" value=""
	value=$(stat -c '%d:%i:%s' "$path" 2>/dev/null || true)
	[[ -n "$value" ]] || value=$(stat -f '%d:%i:%z' "$path" 2>/dev/null || true)
	[[ -n "$value" ]] && printf '%s\n' "$value"
}

loom_lifecycle_transcript_evidence() {
	local path="$1" hook="$2" before="" after="" bytes="" final_record=""
	local final_bytes="" digest="" newline_count="" status=0
	loom_lifecycle_plain_path "$path" file || return 1
	before=$(loom_lifecycle_stat_fingerprint "$path") || return 1
	bytes=$(wc -c <"$path" 2>/dev/null) || return 1
	[[ "$bytes" =~ ^[0-9]+$ ]] && ((bytes > 0)) || return 1
	newline_count=$(tail -c 1 "$path" 2>/dev/null | wc -l) || return 1
	[[ "$newline_count" =~ ^[[:space:]]*1[[:space:]]*$ ]] || return 1
	final_record=$(tail -n 1 "$path" 2>/dev/null) || return 1
	[[ -n "$final_record" ]] || return 1
	final_bytes=$(printf '%s' "$final_record" | wc -c) || return 1
	[[ "$final_bytes" =~ ^[[:space:]]*[0-9]+[[:space:]]*$ ]] || return 1
	final_bytes=${final_bytes//[[:space:]]/}
	((final_bytes < 1048576)) || return 1
	printf '%s' "$final_record" | jq -e 'type == "object"' >/dev/null 2>&1
	status=$?
	if ((status != 0)); then
		if ((status == 2 || status == 3)); then
			loom_lifecycle_log_jq_failure "$hook" "validating the final transcript record" "$status"
			return 2
		fi
		return 1
	fi
	digest=$(printf '%s' "$final_record" | sha256sum 2>/dev/null) || return 1
	digest=${digest%% *}
	[[ "$digest" =~ ^[0-9a-f]{64}$ ]] || return 1
	after=$(loom_lifecycle_stat_fingerprint "$path") || return 1
	loom_lifecycle_plain_path "$path" file || return 1
	[[ "$before" == "$after" ]] || return 1
	LIFECYCLE_TRANSCRIPT_BYTES=${bytes//[[:space:]]/}
	LIFECYCLE_FINAL_DIGEST="sha256:$digest"
}

loom_lifecycle_journal_ready() {
	local journal="$1" bytes="" newline_count=""
	if [[ -e "$journal" || -L "$journal" ]]; then
		loom_lifecycle_plain_path "$journal" file || return 1
		bytes=$(wc -c <"$journal" 2>/dev/null) || return 1
		[[ "$bytes" =~ ^[0-9]+$ ]] && ((bytes <= 8388608)) || return 1
		if ((bytes > 0)); then
			newline_count=$(tail -c 1 "$journal" 2>/dev/null | wc -l) || return 1
			[[ "$newline_count" =~ ^[[:space:]]*1[[:space:]]*$ ]] || return 1
		fi
	fi
	[[ ! -L "$journal" && ! -d "$journal" ]]
}

loom_lifecycle_append() {
	local work="$1" stage="$2" session="$3" record="$4" hook="$5"
	local directory="$work/subagents/$stage" journal="$work/subagents/$stage/lifecycle.jsonl"
	local lock="${journal}.lock" result=1
	if [[ ! -e "$work/subagents" && ! -L "$work/subagents" ]]; then
		mkdir -m 700 "$work/subagents" 2>/dev/null || true
	fi
	if loom_lifecycle_plain_path "$work/subagents" dir &&
		[[ ! -e "$directory" && ! -L "$directory" ]]; then
		mkdir -m 700 "$directory" 2>/dev/null || true
	fi
	loom_lifecycle_plain_path "$work/subagents" dir &&
		loom_lifecycle_plain_path "$directory" dir || {
		loom_debug "$hook: skipping lifecycle append - journal directory is unsafe"; return 1;
	}
	loom_lifecycle_journal_ready "$journal" || {
		loom_debug "$hook: skipping lifecycle append - journal is unsafe, oversized, or torn"; return 1;
	}
	if ! loom_heartbeat_lock_acquire "$lock"; then
		loom_debug "$hook: skipping lifecycle append - lock unavailable"
		return 1
	fi
	if loom_lifecycle_stage_binding "$work" "$stage" "$session" &&
		loom_lifecycle_journal_ready "$journal" && printf '%s\n' "$record" >>"$journal" 2>/dev/null; then
		chmod 600 "$journal" 2>/dev/null || true
		result=0
	else
		loom_debug "$hook: skipping lifecycle append - ownership changed or append failed"
	fi
	loom_heartbeat_lock_release "$lock"
	return "$result"
}

loom_lifecycle_read_heartbeat_fields() {
	local heartbeat="$1" hook="$2" status=0
	LIFECYCLE_HEARTBEAT_TOKENS=$(jq -r '.context_tokens // empty' "$heartbeat" 2>/dev/null)
	status=$?
	if ((status != 0)); then
		loom_lifecycle_log_jq_failure "$hook" "reading optional heartbeat context tokens" "$status"
		return 1
	fi
	LIFECYCLE_HEARTBEAT_TRANSCRIPT=$(jq -r '.transcript_path // empty' "$heartbeat" 2>/dev/null)
	status=$?
	if ((status != 0)); then
		loom_lifecycle_log_jq_failure "$hook" "reading the optional heartbeat transcript" "$status"
		return 1
	fi
}

loom_lifecycle_refresh_heartbeat() {
	local work="$1" stage="$2" session="$3" activity="$4" hook="$5"
	local directory="$work/heartbeat" heartbeat="$work/heartbeat/$stage.json" lock=""
	local tokens="" transcript="" timestamp="" json="" status=0 result=1
	if [[ ! -e "$directory" && ! -L "$directory" ]]; then
		mkdir -m 700 "$directory" 2>/dev/null || true
	fi
	loom_lifecycle_plain_path "$directory" dir || {
		loom_debug "$hook: skipping heartbeat refresh - heartbeat directory is unsafe"; return 1;
	}
	chmod 700 "$directory" 2>/dev/null || true
	lock="${heartbeat}.lock"
	if ! loom_heartbeat_lock_acquire "$lock"; then return 1; fi
	if ! loom_lifecycle_stage_binding "$work" "$stage" "$session"; then
		loom_debug "$hook: skipping stale heartbeat refresh for session $session"
		loom_heartbeat_lock_release "$lock"; return 1
	fi
	if [[ -e "$heartbeat" || -L "$heartbeat" ]]; then
		if ! loom_lifecycle_plain_path "$heartbeat" file; then
			loom_debug "$hook: skipping heartbeat refresh - heartbeat file is unsafe"
			loom_heartbeat_lock_release "$lock"; return 1
		fi
		LIFECYCLE_HEARTBEAT_TOKENS=""; LIFECYCLE_HEARTBEAT_TRANSCRIPT=""
		if ! loom_lifecycle_read_heartbeat_fields "$heartbeat" "$hook"; then
			loom_heartbeat_lock_release "$lock"; return 1
		fi
		tokens="$LIFECYCLE_HEARTBEAT_TOKENS"; transcript="$LIFECYCLE_HEARTBEAT_TRANSCRIPT"
	fi
	timestamp=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z" 2>/dev/null || true)
	json=$(jq -nc --arg stage_id "$stage" --arg session_id "$session" --arg timestamp "$timestamp" \
		--arg activity "$activity" --arg tokens "$tokens" --arg transcript "$transcript" '
		{stage_id:$stage_id,session_id:$session_id,timestamp:$timestamp,
		 context_tokens:(if ($tokens|test("^[0-9]+$")) then ($tokens|tonumber) else null end),
		 transcript_path:(if $transcript == "" then null else $transcript end),
		 last_tool:null,activity:$activity}' 2>/dev/null)
	status=$?
	if ((status != 0)); then
		loom_lifecycle_log_jq_failure "$hook" "constructing heartbeat JSON" "$status"
	elif [[ -n "$timestamp" && -n "$json" ]] && loom_heartbeat_atomic_write "$heartbeat" "$json"; then
		result=0
	else
		loom_debug "$hook: skipping heartbeat refresh - JSON construction or replacement failed"
	fi
	loom_heartbeat_lock_release "$lock"
	return "$result"
}

loom_lifecycle_read_input() {
	if command -v gtimeout &>/dev/null; then
		gtimeout 1 cat 2>/dev/null || true
	elif command -v timeout &>/dev/null; then
		timeout 1 cat 2>/dev/null || true
	else
		cat 2>/dev/null || true
	fi
}
