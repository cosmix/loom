#!/usr/bin/env bash
# _read_ledger.sh - TSV ledger read/write/cap helpers for
# hooks/_read_discipline.sh (Task C/D's read- and poll-repeat counters).
#
# Split out of _read_discipline.sh purely for size - CLAUDE.md rule 17's
# 400-line file cap left no room for this module once the ledger-capping and
# deny-liveness fixes landed there. A SOURCED LIBRARY like _common.sh and
# _read_discipline.sh - never registered as a PreToolUse entry - sourced BY
# _read_discipline.sh (not by the hooks directly), so read-guard.sh and
# poll-guard.sh get it transitively the same way they already get
# _common.sh's helpers through _read_discipline.sh's own use of them.
#
# Bash 3.2+ compatible (macOS default) - see _common.sh's header for the
# constraints this implies.

if [[ "${_LOOM_READ_LEDGER_LOADED:-}" == "1" ]]; then
	return 0
fi
_LOOM_READ_LEDGER_LOADED=1

# _LOOM_LEDGER_MAX_ROWS - cap on a ledger's row count, enforced on every
# append. The repeat-read/repeat-poll counters below only ever need to know
# whether a path/key occurred once, twice, or "3+"/"5+" times this session -
# a few hundred rows is ample headroom for that, and capping keeps both the
# ledger FILE and the O(n) `while read` scan every counter does per call
# bounded, instead of both growing without limit over a long session.
_LOOM_LEDGER_MAX_ROWS=300

# _loom_ledger_cap <file> - best-effort: once <file> exceeds
# _LOOM_LEDGER_MAX_ROWS lines, trim it down to its most recent
# _LOOM_LEDGER_MAX_ROWS. Any failure leaves <file> as-is - a failed trim must
# never lose the row just appended or change a hook's decision.
_loom_ledger_cap() {
	local file="$1" n tmp
	n=$(wc -l <"$file" 2>/dev/null | tr -d '[:space:]')
	[[ -n "$n" ]] && ((n > _LOOM_LEDGER_MAX_ROWS)) || return 0
	tmp=$(mktemp "${file}.XXXXXX" 2>/dev/null) || return 0
	tail -n "$_LOOM_LEDGER_MAX_ROWS" "$file" >"$tmp" 2>/dev/null && mv "$tmp" "$file" 2>/dev/null
	rm -f "$tmp" 2>/dev/null
	chmod 600 "$file" 2>/dev/null || true
	return 0
}

# _loom_ledger_append <file> <field>... - append one TSV row (each <field>
# tab/newline-stripped, joined by tabs) plus a trailing UTC timestamp column,
# then cap the ledger (_loom_ledger_cap).
#
# Best-effort: every failure path returns 0 rather than propagating an
# error - a failed ledger write must never change a hook's decision or exit
# code. Directory creation/permissions follow the loom/hooks convention
# (plain mkdir/chmod, never the Rust CLI - .work is a symlink inside a
# worktree; see subagent-stop.sh's header). A symlinked ledger DIRECTORY or
# FILE is refused outright, matching subagent-stop.sh's own `[[ ! -L "$FILE"
# ]]` guard - `mkdir -p` succeeds silently through a pre-planted directory
# symlink (e.g. a shared /tmp with TMPDIR unset), which would otherwise
# redirect every ledger write wherever that symlink points.
_loom_ledger_append() {
	local file="$1"
	shift
	local dir
	dir="$(dirname "$file")"
	[[ -L "$dir" ]] && return 0
	mkdir -p -m 700 "$dir" 2>/dev/null || return 0
	chmod 700 "$dir" 2>/dev/null || true
	[[ -L "$file" ]] && return 0

	local field row=""
	for field in "$@"; do
		field="${field//$'\t'/}"
		field="${field//$'\n'/}"
		if [[ -z "$row" ]]; then
			row="$field"
		else
			row="${row}"$'\t'"${field}"
		fi
	done
	row="${row}"$'\t'"$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")"
	printf '%s\n' "$row" >>"$file" 2>/dev/null || return 0
	chmod 600 "$file" 2>/dev/null || true
	_loom_ledger_cap "$file"
	return 0
}

# _loom_reads_full_count_and_ts <ledger> <path> - populate
# LOOM_READS_FULL_COUNT (prior "full"-kind rows for <path>) and
# LOOM_READS_FULL_FIRST_TS (the first such row's timestamp, or empty).
_loom_reads_full_count_and_ts() {
	local file="$1" path="$2" p k l t
	LOOM_READS_FULL_COUNT=0
	LOOM_READS_FULL_FIRST_TS=""
	[[ -r "$file" ]] || return 0
	while IFS=$'\t' read -r p k l t; do
		[[ "$p" == "$path" && "$k" == "full" ]] || continue
		LOOM_READS_FULL_COUNT=$((LOOM_READS_FULL_COUNT + 1))
		[[ -z "$LOOM_READS_FULL_FIRST_TS" ]] && LOOM_READS_FULL_FIRST_TS="$t"
	done <"$file"
	return 0
}

# _loom_reads_range_count <ledger> <path> <lines> - echo the count of prior
# "range"-kind rows for <path> whose recorded range exactly equals <lines>.
_loom_reads_range_count() {
	local file="$1" path="$2" lines="$3" count=0 p k l t
	[[ -r "$file" ]] || {
		printf '0'
		return 0
	}
	while IFS=$'\t' read -r p k l t; do
		[[ "$p" == "$path" && "$k" == "range" && "$l" == "$lines" ]] && count=$((count + 1))
	done <"$file"
	printf '%s' "$count"
	return 0
}

# _loom_polls_count <ledger> <key> - echo the count of prior rows whose
# first column exactly equals <key>.
_loom_polls_count() {
	local file="$1" key="$2" count=0 l t
	[[ -r "$file" ]] || {
		printf '0'
		return 0
	}
	while IFS=$'\t' read -r l t; do
		[[ "$l" == "$key" ]] && count=$((count + 1))
	done <"$file"
	printf '%s' "$count"
	return 0
}
