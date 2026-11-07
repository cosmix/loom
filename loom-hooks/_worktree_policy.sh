#!/usr/bin/env bash
# Shared worktree policy data, loaded from worktree-policy.json. Tokenization
# stays with the caller; loom_policy_git_tokens resolves git directory targets
# through _containment.sh (loom_normalize_path, loom_path_class), which every
# caller has through _common.sh.

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

# loom_load_worktree_policy [<message-prefix>] - load the policy globals. The
# prefix (a guard's `[<guard>/<rule>] ` tag) opens each failure message.
loom_load_worktree_policy() {
    local records key first second third tag="${1:-}"
    records=$(loom_worktree_policy_records "$(dirname "${BASH_SOURCE[0]}")/worktree-policy.json") || {
        echo "${tag}LOOM: BLOCKED - shared worktree policy is missing or malformed" >&2
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
    loom_worktree_policy_patterns_valid "$tag"
}

loom_worktree_policy_patterns_valid() {
    local pattern
    for pattern in "$LOOM_POLICY_PARENT_PAIR" "$LOOM_POLICY_PARENT_COMPONENT" \
        "$LOOM_POLICY_WORKTREE_MARKER" "$LOOM_POLICY_STATE_MARKER"; do
        [[ "" =~ $pattern ]] && continue
        if [[ $? -eq 2 ]]; then
            echo "${1:-}LOOM: BLOCKED - shared worktree policy contains an invalid pattern" >&2
            return 1
        fi
    done
    return 0
}

# _loom_git_value_option <token> - git global options whose separate value
# word never retargets git; the value is skipped while finding the subcommand.
_loom_git_value_option() {
    case "$1" in -c | --namespace | --config-env | --attr-source) return 0 ;; esac
    return 1
}

# _loom_git_target_ok <abs> - a retarget is allowed only into the current
# worktree or the session scratch directory.
_loom_git_target_ok() {
    case "$(loom_path_class "$1")" in worktree | scratch) return 0 ;; esac
    return 1
}

# _loom_git_global_overrides <git-index> - fill _lg_chdirs (each `-C` value,
# in order) and _lg_targets (every other policy flag's value) from the global
# options between LOOM_TOKENS[git-index] and the subcommand word, the first
# word not starting with `-` that is no option's value. `git log -C` and
# `git rev-parse --git-dir` are therefore no overrides. Returns 1 when a
# policy flag has no value word (git refuses it; the caller fails closed).
_loom_git_global_overrides() {
    local n=${#LOOM_TOKENS[@]} k="$1" tok record flag mode fallback found value
    while ((++k < n)); do
        tok="${LOOM_TOKENS[$k]}"
        [[ "$tok" != "%%SEP%%" && "$tok" == -* ]] || return 0
        found=""
        for record in "${LOOM_POLICY_OPTIONS[@]}"; do
            IFS=$'\t' read -r flag mode fallback <<<"$record"
            if [[ "$tok" == "$flag" ]]; then
                found=next
            elif [[ "$mode" != exact && "$tok" == "$flag="* ]]; then
                found=inline value="${tok#*=}"
            fi
            [[ -z "$found" ]] || break
        done
        if [[ -z "$found" ]]; then
            _loom_git_value_option "$tok" || continue
            ((k + 1 < n)) && [[ "${LOOM_TOKENS[$((k + 1))]}" != "%%SEP%%" ]] || return 0
            k=$((k + 1))
            continue
        fi
        if [[ "$found" == next ]]; then
            ((k + 1 < n)) && [[ "${LOOM_TOKENS[$((k + 1))]}" != "%%SEP%%" ]] || return 1
            k=$((k + 1))
            value="${LOOM_TOKENS[$k]}"
        fi
        if [[ "$flag" == -C ]]; then _lg_chdirs+=("$value"); else _lg_targets+=("$value"); fi
    done
    return 0
}

# _loom_git_segment_retargets <git-index> - 0 (block) when the git invocation
# at LOOM_TOKENS[git-index] carries a global override that does not resolve
# into the worktree or scratch. git applies `-C` in order, so each relative
# `-C` resolves against the previous one and --git-dir/--work-tree against
# the last. Fails closed (git writes refs where it is pointed): an unknown or
# unset LOOM_TOKEN_CWD (a caller that ran no loom_track_cwd keeps the old
# block), a failed normalization (any `$` but an expandable
# $LOOM_SCRATCH_DIR, or glob/brace/escape text) or any other class blocks. Known limit: the tokenizer
# drops quotes, so a single-quoted '$LOOM_SCRATCH_DIR' reads as the variable.
_loom_git_segment_retargets() {
    local j="$1" cur value
    local -a _lg_chdirs=() _lg_targets=()
    _loom_git_global_overrides "$j" || return 0
    ((${#_lg_chdirs[@]} + ${#_lg_targets[@]} > 0)) || return 1
    [[ -n "${LOOM_TOKEN_CWD[$j]:-}" ]] || return 0
    cur="${LOOM_TOKEN_CWD[$j]}"
    for value in ${_lg_chdirs[@]+"${_lg_chdirs[@]}"}; do
        cur=$(loom_normalize_word "$cur" "$value") || return 0
        _loom_git_target_ok "$cur" || return 0
    done
    for value in ${_lg_targets[@]+"${_lg_targets[@]}"}; do
        value=$(loom_normalize_word "$cur" "$value") || return 0
        _loom_git_target_ok "$value" || return 0
    done
    return 1
}

# loom_policy_git_tokens - 0 when the tokenized command retargets git outside
# the worktree (global -C/--git-dir/--work-tree, per the policy's
# git_options), assigns a git_env variable, or runs an opaque command (eval).
loom_policy_git_tokens() {
    local n=${#LOOM_TOKENS[@]} i=0 at=1 j name
    while ((i < n)); do
        if [[ "${LOOM_TOKENS[$i]}" == "%%SEP%%" ]]; then
            at=1 i=$((i + 1))
            continue
        fi
        if ((at)) && j=$(loom_tokens_command_word_index "$i") &&
            [[ "${LOOM_TOKENS[$j]##*/}" == git ]] && _loom_git_segment_retargets "$j"; then
            return 0
        fi
        at=0 i=$((i + 1))
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
