#!/usr/bin/env bash
# _read_ledger.sh - TSV ledger read/write/cap helpers for
# loom-hooks/_read_discipline.sh (poll counters, the sibling-read scan
# across one session directory's ledgers, and the deny-once read gate's
# denial ledger). It sources _bounded.sh, so
# post-tool-use.sh and subagent-stop.sh, which source this file, get
# loom_run_bounded through it.
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

source "$(dirname "${BASH_SOURCE[0]}")/_bounded.sh"
# _LOOM_LEDGER_MAX_ROWS - cap on a ledger's row count, enforced on every
# append. Poll counters and the sibling-read scan read it. A few hundred rows
# is ample diagnostic headroom and keeps both the ledger file and every O(n)
# scan bounded over a long session.
_LOOM_LEDGER_MAX_ROWS=300

# _loom_read_skip_extension <path> - Return 0 when <path>'s extension is a
# binary/image format that text-oriented read advice skips.
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
	n=$(wc -l 2>/dev/null <"$file" | tr -d '[:space:]')
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
# inside a worktree; see subagent-stop.sh's header). A symlinked ledger
# DIRECTORY, its parent (the per-kind root holding one directory per
# session), or the FILE is refused outright, matching subagent-stop.sh's own
# `[[ ! -L "$FILE" ]]` guard - `mkdir -p` succeeds silently through a
# pre-planted directory symlink (e.g. a shared /tmp with TMPDIR unset), which
# would otherwise redirect every ledger write wherever that symlink points.
# The parent is created 0700 too: outside a stage it is $TMPDIR/loom-reads,
# shared by every session of the user.
_loom_ledger_append() {
	local file="$1"
	shift
	local dir="${file%/*}"
	local parent="${dir%/*}"
	[[ -L "$dir" || -L "$parent" ]] && return 0
	if [[ ! -d "$dir" ]]; then
		mkdir -p -m 700 "$parent" 2>/dev/null || true
		mkdir -p -m 700 "$dir" 2>/dev/null || return 0
	fi
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
	# The group's 2>/dev/null is in place before `>>` opens the file; a
	# trailing `>>"$file" 2>/dev/null` would print bash's own "Permission
	# denied" for an unwritable ledger, since redirections apply left to right.
	{ printf '%s\n' "$row" >>"$file"; } 2>/dev/null || return 0
	chmod 600 "$file" 2>/dev/null || true
	_loom_ledger_cap "$file"
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

# Sibling-read advisory. Every agent of one session writes its reads ledger
# into the same session directory (_loom_ledger_file), so a whole-file read
# can be checked against what the other agents already requested whole. Each hit
# appends "<absolute path>\t<lines>\t<agent count>\t<UTC timestamp>" to the
# shared file loom_read_discipline_check names (the session directory's
# _shared.tsv; the latest row per path carries the current count) for an
# orchestrator to consult before its next round of briefs. No hook prints it.
_LOOM_SIBLING_READ_MIN_LINES=200
_LOOM_SIBLING_LEDGER_MAX=20

# _loom_mtime_iso <path> - print <path>'s mtime in the ledger timestamp's
# UTC shape without the fraction (%Y-%m-%dT%H:%M:%S), so a row's timestamp
# column compares against it as a plain string: a row stamped in the same
# second compares greater, i.e. "not older". loom_heartbeat_lock_epoch
# (_common.sh) is the portable stat wrapper; GNU date takes `-d @epoch`,
# BSD date `-r epoch`.
_loom_mtime_iso() {
	local epoch out shape='^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}$'
	epoch=$(loom_heartbeat_lock_epoch "$1") || return 1
	out=$(date -u -d "@${epoch}" +%Y-%m-%dT%H:%M:%S 2>/dev/null || true)
	[[ "$out" =~ $shape ]] || out=$(date -u -r "$epoch" +%Y-%m-%dT%H:%M:%S 2>/dev/null || true)
	[[ "$out" =~ $shape ]] || return 1
	printf '%s' "$out"
}

# _loom_sibling_ledgers <own_ledger> - print up to _LOOM_SIBLING_LEDGER_MAX
# other agents' ledgers from <own_ledger>'s directory, most recently written
# first. Skips <own_ledger>, "_"-prefixed session files (no sanitized agent
# id starts with "_"), and anything not a plain file. BSD ls colours even
# piped output under CLICOLOR_FORCE, hence the unset.
_loom_sibling_ledgers() {
	local own="$1" listing f n=0
	listing=$(
		unset CLICOLOR CLICOLOR_FORCE
		ls -1t -- "${own%/*}"/*.tsv 2>/dev/null || true
	)
	while IFS= read -r f; do
		[[ -n "$f" && "$f" != "$own" && "${f##*/}" != _* ]] || continue
		[[ -f "$f" && ! -L "$f" ]] || continue
		printf '%s\n' "$f"
		n=$((n + 1))
		((n < _LOOM_SIBLING_LEDGER_MAX)) || return 0
	done <<<"$listing"
	return 0
}

# _loom_sibling_full_read <path> <abs_path> <own_ledger> - one awk pass over
# the sibling ledgers (and <own_ledger>) for `full` rows of <path>, as given
# or absolute, stamped no earlier than the file's mtime. Prints
# "<ledger>\t<lines>\t<sibling count>" for the most recent sibling row. Fails
# when no sibling has one, or when <own_ledger> already has one: this agent
# has read the current file whole itself, so it was told once already. A torn last row lacks its timestamp and never matches.
_loom_sibling_full_read() {
	local path="$1" abs="$2" own="$3" mtime f
	local -a files=()
	mtime=$(_loom_mtime_iso "$path") || return 1
	while IFS= read -r f; do
		[[ -n "$f" ]] && files+=("$f")
	done < <(_loom_sibling_ledgers "$own")
	((${#files[@]} > 0)) || return 1
	[[ -f "$own" && ! -L "$own" ]] && files=("$own" "${files[@]}")
	LOOM_SIB_PATH="$path" LOOM_SIB_ABS="$abs" LOOM_SIB_MTIME="$mtime" LOOM_SIB_OWN="$own" awk '
		BEGIN { FS = "\t"; p = ENVIRON["LOOM_SIB_PATH"]; a = ENVIRON["LOOM_SIB_ABS"]
			m = ENVIRON["LOOM_SIB_MTIME"]; own = ENVIRON["LOOM_SIB_OWN"] }
		NF >= 4 && $2 == "full" && ($1 == p || $1 == a) && $4 >= m {
			if (FILENAME == own) { mine = 1; next }
			if (!(FILENAME in seen)) { seen[FILENAME] = 1; n++ }
			if ($4 >= t) { t = $4; f = FILENAME; l = $3 }
		}
		END { if (mine || n == 0) exit 1; printf "%s\t%s\t%d\n", f, l, n }
	' "${files[@]}" 2>/dev/null
}

# _loom_read_sibling_request <path> <kind> <lines> <own_ledger> <shared_file>
# - for a whole-file read of a regular file above _LOOM_SIBLING_READ_MIN_LINES
# lines that a sibling agent already requested whole, unchanged since: append the
# <shared_file> row and print the advisory. Prints nothing and fails
# otherwise. Run it BEFORE this read's own ledger append, or the fresh own
# row mutes it.
_loom_read_sibling_request() {
	local path="$1" kind="$2" lines="$3" own="$4" shared="$5" abs hit sib sib_lines count who more=""
	[[ "$kind" == "full" && "$lines" =~ ^[0-9]+$ && -f "$path" ]] || return 1
	((lines > _LOOM_SIBLING_READ_MIN_LINES)) || return 1
	abs="$path"
	[[ "$abs" == /* ]] || abs="${PWD%/}/${abs#./}"
	hit=$(_loom_sibling_full_read "$path" "$abs" "$own") || return 1
	IFS=$'\t' read -r sib sib_lines count <<<"$hit"
	_loom_ledger_append "$shared" "$abs" "$lines" "$((count + 1))"
	sib="${sib##*/}"
	sib="${sib%.tsv}"
	who="agent ${sib}"
	[[ "$sib" == "main" ]] && who="the main agent"
	((count > 1)) && more=" and $((count - 1)) other agent(s)"
	printf '%s (%s lines) was requested whole by %s%s (an attempt, not proof of delivery) and is unchanged since - run `loom map --outline %s`, then Read only the range you need with offset/limit.' \
		"$path" "$sib_lines" "$who" "$more" "$path"
}

# Deny-once read gate for rule 1 (_read_discipline.sh). Inside a live stage the
# first whole read of a large file by one agent is denied with the outline, and
# that agent's next whole read of the same file passes. The key is the agent
# and the file's absolute path, so a Bash `cat ../x` and a Read of the absolute
# path share one denial. These functions call is_ancestor (_identity.sh),
# loom_read_gate_enabled (_containment.sh), loom_hook_note_warn and
# _loom_read_discipline_verdict1 (_read_discipline.sh): only
# loom_read_discipline_check calls them, after all three are sourced.

# _loom_read_gate_live - Return 0 when the gate may deny: LOOM_STAGE_ID and
# LOOM_MAIN_AGENT_PID set, the main agent a live ancestor of this process (the
# gate spawn-guard.sh applies; LOOM_STAGE_ID alone leaks into plain sessions),
# and `[hooks] read_gate` not set to false.
_loom_read_gate_live() {
	[[ -n "${LOOM_STAGE_ID:-}" && -n "${LOOM_MAIN_AGENT_PID:-}" ]] || return 1
	is_ancestor "$LOOM_MAIN_AGENT_PID" || return 1
	loom_read_gate_enabled
}

# _loom_read_abs_path <path> - echo <path> with its directory made absolute
# through `cd` and `pwd -P` in a subshell. Fails when the directory cannot be
# entered.
_loom_read_abs_path() {
	local path="$1" dir="." base="$1"
	if [[ "$path" == */* ]]; then
		dir="${path%/*}"
		base="${path##*/}"
		[[ -n "$dir" ]] || dir="/"
	fi
	dir=$(CDPATH='' cd -- "$dir" 2>/dev/null && pwd -P) || return 1
	printf '%s/%s' "${dir%/}" "$base"
}

# _loom_read_denial_record <ledger> <abs_path> - append the denial row and
# return 0 only when a re-count proves it is there. A full ledger is refused
# up front: _loom_ledger_append trims to the newest rows, so appending would
# evict an earlier denial and deny that path a second time. The re-count is
# the only proof of a write, since _loom_ledger_append swallows every failure
# (an unwritable directory, a symlinked one).
_loom_read_denial_record() {
	local ledger="$1" abs="$2" rows=0
	if [[ -r "$ledger" ]]; then
		rows=$(wc -l 2>/dev/null <"$ledger" | tr -d '[:space:]') || rows=0
	fi
	((${rows:-0} < _LOOM_LEDGER_MAX_ROWS)) || return 1
	_loom_ledger_append "$ledger" "$abs"
	(($(_loom_polls_count "$ledger" "$abs") > 0))
}

# _loom_read_rule1_warn <path> <lines> - queue rule 1's message as a warning.
_loom_read_rule1_warn() {
	_loom_read_discipline_verdict1 "$1" "$2"
	loom_hook_note_warn "$LOOM_RD_V1_MSG"
}

# loom_read_deny_once <path> <lines> <agent_id> <fallback_sid> - apply rule 1
# to a whole read of <path> (<lines> long). Outside a live stage, or with
# `read_gate = false`, the message is only a warning. Inside one, a repeat
# this agent was already denied passes silently, checked before the outline
# runs so the repeat never pays for `loom map --outline`; a first read is
# recorded in the "read-denials" ledger and denied (exit 2). A denial that
# cannot be recorded would deny forever, so it is a warning instead.
loom_read_deny_once() {
	local path="$1" lines="$2" agent_id="$3" fallback_sid="$4" abs ledger
	if ! _loom_read_gate_live; then
		_loom_read_rule1_warn "$path" "$lines"
		return 0
	fi
	if ! abs=$(_loom_read_abs_path "$path"); then
		_loom_read_rule1_warn "$path" "$lines"
		return 0
	fi
	ledger=$(_loom_ledger_file "read-denials" "$agent_id" "$fallback_sid")
	if (($(_loom_polls_count "$ledger" "$abs") > 0)); then
		return 0
	fi
	_loom_read_discipline_verdict1 "$path" "$lines"
	if ! _loom_read_denial_record "$ledger" "$abs"; then
		loom_hook_note_warn "$LOOM_RD_V1_MSG"
		return 0
	fi
	printf '⛔ BLOCKED: %s\nRepeating this read is allowed if you need the whole file.\n' \
		"$LOOM_RD_V1_MSG" >&2
	exit 2
}
