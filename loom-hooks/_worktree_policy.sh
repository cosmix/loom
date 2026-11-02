#!/usr/bin/env bash
# Shared lexical policy data. Context, tokenization and filesystem resolution
# remain with the caller.

loom_worktree_policy_records() {
    jq -er '
      def decision: . == "allow" or . == "deny";
      def text: type == "string" and length > 0 and (test("[\\t\\r\\n]") | not);
      select(
        ([.parent_pair, .parent_component, .worktree_marker, .state_marker] | all(text)) and
        (.git_options | type == "array" and length > 0 and all(
          (.flag | text and test("^[-A-Za-z]+$")) and
          (.token == "exact" or .token == "equals-or-exact") and
          (.fallback == "value" or .fallback == "equals-or-value"))) and
        (.git_env | type == "array" and all(text and test("^[A-Z_]+$"))) and
        (.opaque_commands | type == "array" and all(text and test("^[a-z]+$"))) and
        (.shared_state | type == "object" and (to_entries | all(
          (.key | test("^[A-Za-z0-9_-]+$")) and
          (.value.live_write | decision)))) and
        (.defaults.live_write | decision)
      ) |
      ["parent_pair", .parent_pair], ["parent_component", .parent_component],
      ["worktree_marker", .worktree_marker], ["state_marker", .state_marker],
      (.git_options[] | ["option", .flag, .token, .fallback]),
      (.git_env[] | ["env", .]), (.opaque_commands[] | ["opaque", .]),
      (.shared_state | to_entries[] | ["state", .key, .value.live_write]),
      ["default", .defaults.live_write] | join("\t")
    ' "$1"
}

loom_load_worktree_policy() {
    local records key first second third
    records=$(loom_worktree_policy_records "$(dirname "${BASH_SOURCE[0]}")/worktree-policy.json") || {
        echo "LOOM: BLOCKED - shared worktree policy is missing or malformed" >&2
        return 1
    }
    LOOM_POLICY_OPTIONS=() LOOM_POLICY_ENV=() LOOM_POLICY_OPAQUE=() LOOM_POLICY_STATE=()
    while IFS=$'\t' read -r key first second third; do
        case "$key" in
        parent_pair) LOOM_POLICY_PARENT_PAIR="$first" ;;
        parent_component) LOOM_POLICY_PARENT_COMPONENT="$first" ;;
        worktree_marker) LOOM_POLICY_WORKTREE_MARKER="$first" ;;
        state_marker) LOOM_POLICY_STATE_MARKER="$first" ;;
        option) LOOM_POLICY_OPTIONS+=("$first"$'\t'"$second"$'\t'"$third") ;;
        env) LOOM_POLICY_ENV+=("$first") ;;
        opaque) LOOM_POLICY_OPAQUE+=("$first") ;;
        state) LOOM_POLICY_STATE+=("$first"$'\t'"$second") ;;
        default) LOOM_POLICY_STATE_DEFAULT="$first" ;;
        esac
    done <<<"$records"
    loom_worktree_policy_patterns_valid
}

loom_worktree_policy_patterns_valid() {
    local pattern
    for pattern in "$LOOM_POLICY_PARENT_PAIR" "$LOOM_POLICY_PARENT_COMPONENT" \
        "$LOOM_POLICY_WORKTREE_MARKER" "$LOOM_POLICY_STATE_MARKER"; do
        [[ "" =~ $pattern ]] && continue
        if [[ $? -eq 2 ]]; then
            echo "LOOM: BLOCKED - shared worktree policy contains an invalid pattern" >&2
            return 1
        fi
    done
    return 0
}

loom_policy_git_tokens() {
    local record flag mode fallback pattern name
    for record in "${LOOM_POLICY_OPTIONS[@]}"; do
        IFS=$'\t' read -r flag mode fallback <<<"$record"
        pattern="$flag"
        [[ "$mode" == exact ]] || pattern="$flag(=.*)?"
        loom_tokens_cmd_has_arg git "$pattern" && return 0
    done
    for name in ${LOOM_POLICY_ENV[@]+"${LOOM_POLICY_ENV[@]}"}; do
        loom_tokens_word_matches "^$name=" && return 0
    done
    for name in ${LOOM_POLICY_OPAQUE[@]+"${LOOM_POLICY_OPAQUE[@]}"}; do
        loom_tokens_invoke "$name" && return 0
    done
    return 1
}

loom_policy_git_fallback() {
    local command="$1" record flag mode fallback pattern name
    for record in "${LOOM_POLICY_OPTIONS[@]}"; do
        IFS=$'\t' read -r flag mode fallback <<<"$record"
        pattern="git[[:space:]]+$flag[[:space:]]"
        [[ "$fallback" == value ]] || pattern="git[[:space:]]+$flag([=[:space:]]|$)"
        [[ "$command" =~ $pattern ]] && return 0
    done
    for name in ${LOOM_POLICY_ENV[@]+"${LOOM_POLICY_ENV[@]}"}; do
        pattern="(^|[[:space:];&|(])$name="
        [[ "$command" =~ $pattern ]] && return 0
    done
    for name in ${LOOM_POLICY_OPAQUE[@]+"${LOOM_POLICY_OPAQUE[@]}"}; do
        pattern="(^|[[:space:];&|(])$name([[:space:]]|$)"
        [[ "$command" =~ $pattern ]] && return 0
    done
    return 1
}

loom_policy_shared_write_allowed() {
    local relative="${1#/}" record directory decision name
    # An allowed namespace grants its descendants, never the namespace root.
    [[ "$relative" == */* ]] || return 1
    directory="${relative%%/*}"
    for record in ${LOOM_POLICY_STATE[@]+"${LOOM_POLICY_STATE[@]}"}; do
        IFS=$'\t' read -r name decision <<<"$record"
        [[ "$directory" == "$name" ]] || continue
        [[ "$decision" == allow ]]
        return $?
    done
    [[ "$LOOM_POLICY_STATE_DEFAULT" == allow ]]
}
