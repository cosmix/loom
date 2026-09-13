#!/usr/bin/env bash
# _read_ledger.sh - TSV ledger read/write/cap helpers for
# loom-hooks/_read_discipline.sh (read-attempt diagnostics and poll counters).
# It also provides the shared bounded runner used by post-tool-use.sh,
# read-guard.sh, spawn-guard.sh, and subagent-start.sh.
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

# Resolve commands through loom's pinned hook PATH when set (LOOM_HOOK_PATH):
# inherited PATH directories can be writable from a sandboxed session.
PATH="${LOOM_HOOK_PATH:-$PATH}"

if [[ "${_LOOM_READ_LEDGER_LOADED:-}" == "1" ]]; then
	return 0
fi
_LOOM_READ_LEDGER_LOADED=1

# loom_run_bounded <seconds> <command...>
# Prefer GNU coreutils on macOS, then Linux timeout, with a plain fallback.
# Shared here so PreToolUse and PostToolUse receipt calls get the same bound.
loom_run_bounded() {
	local seconds="$1"
	shift
	if command -v gtimeout &>/dev/null; then
		gtimeout "$seconds" "$@"
	elif command -v timeout &>/dev/null; then
		timeout "$seconds" "$@"
	else
		"$@"
	fi
}

# _LOOM_LEDGER_MAX_ROWS - cap on a ledger's row count, enforced on every
# append. Read receipts use this only as a bounded overlap prefilter; poll
# counters also scan it. A few hundred rows is ample diagnostic headroom and
# keeps both the ledger file and every O(n) scan bounded over a long session.
_LOOM_LEDGER_MAX_ROWS=300

# _loom_read_skip_extension <path> - Return 0 when <path>'s extension is a
# binary/image format that text-oriented read advice and receipts skip.
# Kept beside _loom_read_receipt_eligible so the PostToolUse hook can use the
# same gate without sourcing the read-discipline policy module.
_loom_read_skip_extension() {
	local path="$1" ext lower
	[[ "$path" == *.* ]] || return 1
	ext="${path##*.}"
	lower=$(printf '%s' "$ext" | tr '[:upper:]' '[:lower:]')
	case "$lower" in
	png | jpg | jpeg | gif | webp | bmp | ico | svg | pdf | zip | gz | tar | bz2 | xz | zst | \
		wasm | so | dylib | dll | exe | bin | o | a | class | jar | mp3 | mp4 | mov | avi | \
		ttf | otf | woff | woff2)
		return 0
		;;
	esac
	return 1
}

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
# (plain mkdir/chmod, never the Rust CLI - the state directory is a symlink
# inside a worktree; see subagent-stop.sh's header). A symlinked ledger DIRECTORY or
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

# _loom_read_attempt_overlaps <ledger> <path> <kind> <lines> - cheap attempt
# ledger prefilter for receipt lookup. It proves neither a returned result nor
# source identity: full reads overlap any attempt for their path; numeric
# ranges overlap when their half-open intervals intersect. The Rust receipt
# check remains authoritative.
_loom_read_ranges_overlap() {
	local left="$1" right="$2" left_start left_end right_start right_end
	[[ "$left" =~ ^([0-9]+)-([0-9]+)?$ ]] || return 1
	left_start="${BASH_REMATCH[1]}"
	left_end="${BASH_REMATCH[2]}"
	[[ "$right" =~ ^([0-9]+)-([0-9]+)?$ ]] || return 1
	right_start="${BASH_REMATCH[1]}"
	right_end="${BASH_REMATCH[2]}"
	if [[ -z "$left_end" ]]; then
		[[ -z "$right_end" ]] || ((right_end > left_start))
	elif [[ -z "$right_end" ]]; then
		((left_end > right_start))
	else
		((left_start < right_end && right_start < left_end))
	fi
}

_loom_read_attempt_overlaps() {
	local file="$1" path="$2" kind="$3" lines="$4" p k l t
	[[ -r "$file" ]] || return 1
	while IFS=$'\t' read -r p k l t; do
		[[ "$p" == "$path" ]] || continue
		[[ "$kind" == "full" || "$k" == "full" ]] && return 0
		_loom_read_ranges_overlap "$lines" "$l" && return 0
	done <"$file"
	return 1
}

# _loom_read_receipt_eligible <path> - shell's inexpensive eligibility gate.
# The Rust adapter repeats its own no-follow, content, size, and generation
# validation; this avoids starting it for known media and non-files.
_loom_read_receipt_eligible() {
	local path="$1"
	[[ -f "$path" && ! -L "$path" ]] || return 1
	! _loom_read_skip_extension "$path"
}

# _loom_read_receipt_run <mode> <payload> - pass the original hook payload to
# the bounded Rust adapter. Failures are deliberately indistinguishable from a
# missing receipt so receipt instrumentation cannot block an ordinary Read.
_loom_read_receipt_run() {
	local mode="$1" payload="$2"
	command -v "${LOOM_BIN:-loom}" &>/dev/null || return 1
	printf '%s' "$payload" | LOOM_HOOK_CONTEXT=1 loom_run_bounded 2 "${LOOM_BIN:-loom}" hook read-receipt "$mode" 2>/dev/null
}

# _loom_read_receipt_proven <payload> - print the CLI's exact proven receipt
# count, or fail. A ledger overlap alone is never a repeat qualification.
_loom_read_receipt_proven() {
	local output count
	output=$(_loom_read_receipt_run --check "$1" || true)
	[[ "$output" =~ ^proven\ ([1-9][0-9]*)$ ]] || return 1
	count="${BASH_REMATCH[1]}"
	printf '%s\n' "$count"
}

# _loom_read_last_full_attempt_at <ledger> <path> - print the timestamp of
# the most recent prior full-read attempt for the repeat advisory.
_loom_read_last_full_attempt_at() {
	local file="$1" path="$2" p kind lines timestamp last=""
	[[ -r "$file" ]] || return 1
	while IFS=$'\t' read -r p kind lines timestamp; do
		[[ "$p" == "$path" && "$kind" == "full" ]] && last="$timestamp"
	done <"$file"
	[[ -n "$last" ]] || return 1
	printf '%s\n' "$last"
}

# _loom_read_receipt_prepare <payload> - best-effort pending-intent creation.
_loom_read_receipt_prepare() {
	_loom_read_receipt_run --prepare "$1" >/dev/null || true
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
