#!/usr/bin/env bash
# credential-guard.sh rule (b): a saved terminal history under a state root is
# closed to the file tools in a loom session, reached directly or through a
# worktree's state-root symlink, in both state-root spellings - and so is a Grep
# or Glob that would walk into the state root: rooted at it, rooted at the
# directory holding it, or filtered by a glob while rooted above it (a glob that
# matches a directory overrides .gitignore). A search of a state-root
# subdirectory, or a plain search of a directory above the state root, still
# runs. A person's own session, which carries no loom identity, is refused
# nothing.
set -euo pipefail
HOOK="$(cd "$(dirname "$0")/.." && pwd)/credential-guard.sh"
FAILED=0

TMPROOT=$(mktemp -d)
trap 'rm -rf "$TMPROOT"' EXIT

# A main repo whose state root holds a transcript, one handoff, and a sibling
# directory sharing the history directory's name prefix. A second repo keeps the
# legacy `.work` spelling. Each worktree reaches its state root the way loom lays
# it out: a symlink to the main repo's.
MAIN="$TMPROOT/main"
LEGACY="$TMPROOT/legacy"
WORKTREE="$MAIN/.worktrees/stage-b"
LEGACY_WORKTREE="$LEGACY/.worktrees/stage-b"
mkdir -p "$MAIN/.loom/work/terminal-history" "$MAIN/.loom/work/handoffs" \
	"$MAIN/.loom/work/memory" "$MAIN/.loom/work/terminal-history-old" "$WORKTREE/.loom"
mkdir -p "$LEGACY/.work/terminal-history" "$LEGACY_WORKTREE"
printf 'stage A transcript\n' >"$MAIN/.loom/work/terminal-history/session-a.terminal"
printf 'handoff\n' >"$MAIN/.loom/work/handoffs/stage-a.md"
printf 'old\n' >"$MAIN/.loom/work/terminal-history-old/x.txt"
printf 'legacy transcript\n' >"$LEGACY/.work/terminal-history/session-a.terminal"
mkdir -p "$MAIN/src"
ln -s "$MAIN/.loom/work" "$WORKTREE/.loom/work"
# Links a stage could create inside its worktree to reach the state root, the
# history directory, and the repo root above both.
ln -s "$MAIN/.loom/work" "$WORKTREE/state-link"
ln -s "$MAIN/.loom/work/terminal-history" "$WORKTREE/history-link"
ln -s "$MAIN" "$WORKTREE/root-link"
ln -s "$LEGACY/.work" "$LEGACY_WORKTREE/.work"

# A third repo whose state root is itself a symlink to a directory outside it: the
# real directory has no `.loom/work` component, so only the session's
# LOOM_WORK_DIR identifies it.
SYMLINKED="$TMPROOT/symlinked"
SYMLINKED_WORKTREE="$SYMLINKED/.worktrees/stage-b"
EXTERNAL="$TMPROOT/external/state"
mkdir -p "$EXTERNAL/terminal-history" "$EXTERNAL/memory" \
	"$SYMLINKED/.loom" "$SYMLINKED_WORKTREE/.loom"
printf 'symlinked transcript\n' >"$EXTERNAL/terminal-history/session-a.terminal"
ln -s "$EXTERNAL" "$SYMLINKED/.loom/work"
ln -s "$SYMLINKED/.loom/work" "$SYMLINKED_WORKTREE/.loom/work"

# check <name> <expected exit> <project dir> <payload> [VAR=value ...]
# The hook runs with <project dir> as its working directory. The extra
# assignments are the session's loom identity; none means the operator's own
# session.
check() {
	local name="$1" expected="$2" project_dir="$3" payload="$4"
	shift 4
	local status=0
	(
		cd "$project_dir"
		env -u LOOM_SESSION_ID -u LOOM_STAGE_ID -u LOOM_WORK_DIR "$@" \
			CLAUDE_PROJECT_DIR="$project_dir" bash "$HOOK" <<<"$payload" >/dev/null 2>&1
	) || status=$?
	if [[ "$status" -eq "$expected" ]]; then
		echo "PASS: $name"
	else
		echo "FAIL: $name - expected exit $expected, got $status"
		FAILED=1
	fi
}

file_payload() {
	printf '{"tool_name":"%s","tool_input":{"file_path":"%s"}}' "$1" "$2"
}

search_payload() {
	printf '{"tool_name":"%s","tool_input":{"pattern":"stage","path":"%s"}}' "$1" "$2"
}

# A Grep with a glob filter, and a Glob with a pattern and no search root.
grep_glob_payload() {
	printf '{"tool_name":"Grep","tool_input":{"pattern":"x","path":"%s","glob":"%s","output_mode":"content"}}' "$1" "$2"
}

glob_pattern_payload() {
	printf '{"tool_name":"Glob","tool_input":{"pattern":"%s"}}' "$1"
}

glob_pattern_path_payload() {
	printf '{"tool_name":"Glob","tool_input":{"pattern":"%s","path":"%s"}}' "$1" "$2"
}

STAGE=(LOOM_SESSION_ID=session-b LOOM_STAGE_ID=stage-b "LOOM_WORK_DIR=$MAIN/.loom/work")
LEGACY_STAGE=(LOOM_SESSION_ID=session-b LOOM_STAGE_ID=stage-b "LOOM_WORK_DIR=$LEGACY/.work")
# LOOM_WORK_DIR as the real directory, and as the symlink that leads to it.
SYMLINKED_REAL=(LOOM_SESSION_ID=session-b LOOM_STAGE_ID=stage-b "LOOM_WORK_DIR=$EXTERNAL")
SYMLINKED_LINK=(LOOM_SESSION_ID=session-b LOOM_STAGE_ID=stage-b "LOOM_WORK_DIR=$SYMLINKED/.loom/work")

check "stage: Read of a transcript by its real path is blocked" \
	2 "$WORKTREE" "$(file_payload Read "$MAIN/.loom/work/terminal-history/session-a.terminal")" "${STAGE[@]}"
check "stage: Read of a transcript through the worktree symlink is blocked" \
	2 "$WORKTREE" "$(file_payload Read "$WORKTREE/.loom/work/terminal-history/session-a.terminal")" "${STAGE[@]}"
check "stage: Read of a transcript through a relative path is blocked" \
	2 "$WORKTREE" "$(file_payload Read ".loom/work/terminal-history/session-a.terminal")" "${STAGE[@]}"
check "stage: Read of a transcript not written yet is blocked" \
	2 "$WORKTREE" "$(file_payload Read "$WORKTREE/.loom/work/terminal-history/session-new.terminal")" "${STAGE[@]}"
check "stage: Grep rooted in the history directory is blocked" \
	2 "$WORKTREE" "$(search_payload Grep "$WORKTREE/.loom/work/terminal-history")" "${STAGE[@]}"
check "stage: Glob rooted in the history directory is blocked" \
	2 "$WORKTREE" "$(search_payload Glob "$MAIN/.loom/work/terminal-history")" "${STAGE[@]}"
check "stage: Edit of a transcript is blocked" \
	2 "$WORKTREE" "$(file_payload Edit "$WORKTREE/.loom/work/terminal-history/session-a.terminal")" "${STAGE[@]}"
check "stage: legacy .work, real path, is blocked" \
	2 "$LEGACY_WORKTREE" "$(file_payload Read "$LEGACY/.work/terminal-history/session-a.terminal")" \
	LOOM_SESSION_ID=session-b LOOM_STAGE_ID=stage-b "LOOM_WORK_DIR=$LEGACY/.work"
check "stage: legacy .work, through the worktree symlink, is blocked" \
	2 "$LEGACY_WORKTREE" "$(file_payload Read "$LEGACY_WORKTREE/.work/terminal-history/session-a.terminal")" \
	LOOM_SESSION_ID=session-b LOOM_STAGE_ID=stage-b "LOOM_WORK_DIR=$LEGACY/.work"

check "stage: Grep rooted at the state root is blocked" \
	2 "$WORKTREE" "$(search_payload Grep "$MAIN/.loom/work")" "${STAGE[@]}"
check "stage: Grep rooted at the state root through the worktree symlink is blocked" \
	2 "$WORKTREE" "$(search_payload Grep "$WORKTREE/.loom/work")" "${STAGE[@]}"
check "stage: Grep rooted at the state root through a trailing slash is blocked" \
	2 "$WORKTREE" "$(search_payload Grep "$WORKTREE/.loom/work/")" "${STAGE[@]}"
check "stage: Grep rooted at the state root through a relative path is blocked" \
	2 "$WORKTREE" "$(search_payload Grep ".loom/work")" "${STAGE[@]}"
check "stage: Glob rooted at the state root is blocked" \
	2 "$WORKTREE" "$(search_payload Glob "$MAIN/.loom/work")" "${STAGE[@]}"
check "stage: Glob rooted at the state root through the worktree symlink is blocked" \
	2 "$WORKTREE" "$(search_payload Glob "$WORKTREE/.loom/work")" "${STAGE[@]}"
check "stage: legacy .work, Grep rooted at the state root, is blocked" \
	2 "$LEGACY_WORKTREE" "$(search_payload Grep "$LEGACY/.work")" "${LEGACY_STAGE[@]}"
check "stage: legacy .work, Glob through the worktree symlink, is blocked" \
	2 "$LEGACY_WORKTREE" "$(search_payload Glob "$LEGACY_WORKTREE/.work")" "${LEGACY_STAGE[@]}"
check "stage: Grep rooted at a state-root subdirectory still runs" \
	0 "$WORKTREE" "$(search_payload Grep "$WORKTREE/.loom/work/memory")" "${STAGE[@]}"
check "stage: Glob rooted at a state-root subdirectory still runs" \
	0 "$WORKTREE" "$(search_payload Glob ".loom/work/memory")" "${STAGE[@]}"
check "stage: a search rooted above the state root still runs" \
	0 "$MAIN" "$(search_payload Grep "$MAIN")" "${STAGE[@]}"
check "stage: a search with no path, rooted at the worktree, still runs" \
	0 "$WORKTREE" '{"tool_name":"Grep","tool_input":{"pattern":"stage"}}' "${STAGE[@]}"
check "stage: Read of a state-root file is untouched by the search rule" \
	0 "$WORKTREE" "$(file_payload Read "$WORKTREE/.loom/work/handoffs/stage-a.md")" "${STAGE[@]}"

# A knowledge stage runs in the main checkout, where `.loom/work` is the real
# directory: the repo root holds the state root, and a glob can reach past the
# .gitignore entry that otherwise hides it.
check "stage: Grep of the repo root with glob * is blocked" \
	2 "$MAIN" "$(grep_glob_payload "$MAIN" '*')" "${STAGE[@]}"
check "stage: Grep of the repo root with glob *.md is blocked" \
	2 "$MAIN" "$(grep_glob_payload "$MAIN" '*.md')" "${STAGE[@]}"
check "stage: Grep of . with glob * is blocked" \
	2 "$MAIN" "$(grep_glob_payload . '*')" "${STAGE[@]}"
check "stage: Grep with no path and a glob, rooted at the project, is blocked" \
	2 "$MAIN" '{"tool_name":"Grep","tool_input":{"pattern":"x","glob":"{.loom,.loom/**}"}}' "${STAGE[@]}"
check "stage: Grep of the directory holding the state root is blocked" \
	2 "$MAIN" "$(search_payload Grep "$MAIN/.loom")" "${STAGE[@]}"
check "stage: Grep of the directory holding the state root, relative, is blocked" \
	2 "$MAIN" "$(search_payload Grep ".loom")" "${STAGE[@]}"
check "stage: Grep of ./.loom is blocked" \
	2 "$MAIN" "$(search_payload Grep "./.loom")" "${STAGE[@]}"
check "stage: Grep of the state root's parent through .. is blocked" \
	2 "$MAIN" "$(search_payload Grep ".loom/work/..")" "${STAGE[@]}"
check "stage: Grep of the worktree's .loom directory is blocked" \
	2 "$WORKTREE" "$(search_payload Grep "$WORKTREE/.loom")" "${STAGE[@]}"
check "stage: Grep of .loom/work/.. in a worktree is blocked" \
	2 "$WORKTREE" "$(search_payload Grep ".loom/work/..")" "${STAGE[@]}"
check "stage: Grep of a symlink to the state root is blocked" \
	2 "$WORKTREE" "$(search_payload Grep "$WORKTREE/state-link")" "${STAGE[@]}"
check "stage: Grep of a symlink to the history directory is blocked" \
	2 "$WORKTREE" "$(search_payload Grep "$WORKTREE/history-link")" "${STAGE[@]}"
check "stage: Read through a symlink to the history directory is blocked" \
	2 "$WORKTREE" "$(file_payload Read "$WORKTREE/history-link/session-a.terminal")" "${STAGE[@]}"
check "stage: Grep of a symlink to the repo root with a glob is blocked" \
	2 "$WORKTREE" "$(grep_glob_payload "$WORKTREE/root-link" '*')" "${STAGE[@]}"
check "stage: Glob with an absolute pattern into the history directory is blocked" \
	2 "$MAIN" "$(glob_pattern_payload "$MAIN/.loom/work/terminal-history/*")" "${STAGE[@]}"
check "stage: Glob with a .. pattern from the repo root is blocked" \
	2 "$MAIN" "$(glob_pattern_path_payload '../*/.loom/work/*' "$MAIN")" "${STAGE[@]}"
check "stage: Grep of the repo root with no glob still runs" \
	0 "$MAIN" "$(search_payload Grep "$MAIN")" "${STAGE[@]}"
check "stage: Grep of . with no glob still runs" \
	0 "$MAIN" "$(search_payload Grep .)" "${STAGE[@]}"
check "stage: Grep of the repo root with an empty glob still runs" \
	0 "$MAIN" "$(grep_glob_payload "$MAIN" '')" "${STAGE[@]}"
check "stage: Grep of a directory beside the state root with glob * still runs" \
	0 "$MAIN" "$(grep_glob_payload "$MAIN/src" '*')" "${STAGE[@]}"
check "stage: Grep of a state-root subdirectory with glob * still runs" \
	0 "$MAIN" "$(grep_glob_payload "$MAIN/.loom/work/memory" '*')" "${STAGE[@]}"
check "stage: Grep of the worktree with glob * still runs" \
	0 "$WORKTREE" "$(grep_glob_payload "$WORKTREE" '*')" "${STAGE[@]}"
check "stage: Grep of a symlink to the repo root with no glob still runs" \
	0 "$WORKTREE" "$(search_payload Grep "$WORKTREE/root-link")" "${STAGE[@]}"
check "stage: Glob with a relative pattern from the repo root still runs" \
	0 "$MAIN" "$(glob_pattern_path_payload '**/*.md' "$MAIN")" "${STAGE[@]}"
check "stage: Glob with an absolute pattern below a directory beside the state root still runs" \
	0 "$MAIN" "$(glob_pattern_path_payload "$MAIN/src/*.rs" "$MAIN/src")" "${STAGE[@]}"

check "symlinked state root: Read of a transcript is blocked" \
	2 "$SYMLINKED_WORKTREE" "$(file_payload Read "$SYMLINKED_WORKTREE/.loom/work/terminal-history/session-a.terminal")" \
	"${SYMLINKED_REAL[@]}"
check "symlinked state root: Grep rooted in the history directory is blocked" \
	2 "$SYMLINKED_WORKTREE" "$(search_payload Grep "$SYMLINKED_WORKTREE/.loom/work/terminal-history")" \
	"${SYMLINKED_LINK[@]}"
check "symlinked state root: Grep rooted at the state root, by its real path, is blocked" \
	2 "$SYMLINKED_WORKTREE" "$(search_payload Grep "$EXTERNAL")" "${SYMLINKED_REAL[@]}"
check "symlinked state root: Grep rooted at the state root through the worktree is blocked" \
	2 "$SYMLINKED_WORKTREE" "$(search_payload Grep "$SYMLINKED_WORKTREE/.loom/work")" "${SYMLINKED_REAL[@]}"
check "symlinked state root: Glob rooted at the state root, LOOM_WORK_DIR a symlink, is blocked" \
	2 "$SYMLINKED_WORKTREE" "$(search_payload Glob "$SYMLINKED_WORKTREE/.loom/work")" "${SYMLINKED_LINK[@]}"
check "symlinked state root: Grep rooted at a subdirectory still runs" \
	0 "$SYMLINKED_WORKTREE" "$(search_payload Grep "$SYMLINKED_WORKTREE/.loom/work/memory")" "${SYMLINKED_REAL[@]}"
check "symlinked state root: a search rooted above it still runs" \
	0 "$SYMLINKED" "$(search_payload Grep "$SYMLINKED")" "${SYMLINKED_REAL[@]}"

check "symlinked state root: Grep of the directory holding the real state root is blocked" \
	2 "$SYMLINKED" "$(search_payload Grep "$TMPROOT/external")" "${SYMLINKED_REAL[@]}"
check "symlinked state root: Grep of the worktree's .loom directory is blocked" \
	2 "$SYMLINKED_WORKTREE" "$(search_payload Grep "$SYMLINKED_WORKTREE/.loom")" "${SYMLINKED_REAL[@]}"
check "symlinked state root: Grep with a glob, rooted above the real state root, is blocked" \
	2 "$SYMLINKED" "$(grep_glob_payload "$TMPROOT" '*')" "${SYMLINKED_REAL[@]}"
check "symlinked state root: Grep of the repo with a glob still runs" \
	0 "$SYMLINKED" "$(grep_glob_payload "$SYMLINKED" '*')" "${SYMLINKED_REAL[@]}"

check "stage: another state-root file is untouched" \
	0 "$WORKTREE" "$(file_payload Read "$WORKTREE/.loom/work/handoffs/stage-a.md")" "${STAGE[@]}"
check "stage: a sibling sharing the directory's name prefix is untouched" \
	0 "$WORKTREE" "$(file_payload Read "$WORKTREE/.loom/work/terminal-history-old/x.txt")" "${STAGE[@]}"
check "stage: a search rooted above the directory still runs" \
	0 "$WORKTREE" "$(search_payload Grep "$WORKTREE/.loom/work/handoffs")" "${STAGE[@]}"
check "stage: a non-file tool with the same payload is untouched" \
	0 "$WORKTREE" "$(file_payload Bash "$MAIN/.loom/work/terminal-history/session-a.terminal")" "${STAGE[@]}"

check "operator: Read of a transcript by its real path is allowed" \
	0 "$MAIN" "$(file_payload Read "$MAIN/.loom/work/terminal-history/session-a.terminal")"
check "operator: Read of a transcript through the worktree symlink is allowed" \
	0 "$WORKTREE" "$(file_payload Read "$WORKTREE/.loom/work/terminal-history/session-a.terminal")"
check "operator: Grep rooted in the history directory is allowed" \
	0 "$MAIN" "$(search_payload Grep "$MAIN/.loom/work/terminal-history")"
check "operator: Grep rooted at the state root is allowed" \
	0 "$MAIN" "$(search_payload Grep "$MAIN/.loom/work")"
check "operator: Glob rooted at the state root through the worktree symlink is allowed" \
	0 "$WORKTREE" "$(search_payload Glob "$WORKTREE/.loom/work")"
check "operator: Grep rooted at a symlinked state root is allowed" \
	0 "$SYMLINKED_WORKTREE" "$(search_payload Grep "$SYMLINKED_WORKTREE/.loom/work")"
check "operator: Grep of the repo root with glob * is allowed" \
	0 "$MAIN" "$(grep_glob_payload "$MAIN" '*')"
check "operator: Grep of the directory holding the state root is allowed" \
	0 "$MAIN" "$(search_payload Grep "$MAIN/.loom")"
check "operator: Glob with an absolute pattern into the history directory is allowed" \
	0 "$MAIN" "$(glob_pattern_payload "$MAIN/.loom/work/terminal-history/*")"
check "operator: Grep of a symlink to the state root is allowed" \
	0 "$WORKTREE" "$(search_payload Grep "$WORKTREE/state-link")"
check "operator: Grep of a symlink to the history directory is allowed" \
	0 "$WORKTREE" "$(search_payload Grep "$WORKTREE/history-link")"
check "operator: Grep of the real state root of a symlinked layout with a glob is allowed" \
	0 "$SYMLINKED" "$(grep_glob_payload "$TMPROOT" '*')"
check "operator: legacy .work Read is allowed" \
	0 "$LEGACY" "$(file_payload Read "$LEGACY/.work/terminal-history/session-a.terminal")"
check "a stale stage id alone does not make a loom session" \
	0 "$MAIN" "$(file_payload Read "$MAIN/.loom/work/terminal-history/session-a.terminal")" \
	LOOM_STAGE_ID=stage-b

exit "$FAILED"
