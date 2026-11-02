#!/usr/bin/env bash
# Stop hook: Enforce commit and stage completion in loom worktrees
#
# This hook runs when Claude finishes responding. In loom worktrees, it reminds:
# 1. All changes must be committed before ending
# 2. Stage must be marked complete (not left in "Executing" state)
#
# Exit codes:
#   0 - Allow Claude to stop (no issues or not in worktree)
#   (Previously used exit 2 to block, now advisory-only due to Claude Code
#    firing Stop hooks during Task tool/subagent waits, not just session exit)

set -euo pipefail

# Drain stdin to prevent blocking (Stop hooks receive JSON from Claude Code)
source "$(dirname "${BASH_SOURCE[0]}")/_bounded.sh"
source "$(dirname "${BASH_SOURCE[0]}")/_common.sh"
loom_read_input 1 >/dev/null 2>&1 || true

WORK_DIR=""
# Debug logging - enabled when LOOM_HOOK_DEBUG=1
debug_log() {
	if [[ "${LOOM_HOOK_DEBUG:-0}" == "1" ]]; then
		printf "[loom-stop] %s\n" "$*" >&2
	fi
}

# Check for uncommitted changes (staged or unstaged)
# Returns: 0 if clean, 1 if dirty
check_git_clean() {
	# Check for any changes (staged, unstaged, or untracked)
	# --porcelain gives machine-readable output
	# Empty output means clean
	local status
	status=$(git status --porcelain 2>/dev/null || echo "")

	if [[ -z "$status" ]]; then
		debug_log "Git status: clean (no uncommitted changes)"
		return 0 # Clean
	else
		debug_log "Git status: dirty (has uncommitted changes)"
		return 1 # Dirty
	fi
}

# Get list of uncommitted changes for error message (first 10 entries).
# Captured before truncation: piping `git status` straight into `head`
# raises SIGPIPE in git once head exits, and under pipefail that 141
# became the hook's exit status.
get_uncommitted_changes() {
	local status
	status=$(git status --porcelain 2>/dev/null || true)
	head -n 10 <<<"$status"
}

# Output advisory warning and allow stop
# Previously this blocked (exit 2), but Claude Code now fires Stop hooks
# during Task tool/subagent waits, not just on session exit. Blocking at
# that point kills the session before the agent can commit.
# Args: $1 = reason string
warn_with_reason() {
	local reason="$1"

	printf '\n' >&2
	printf '%s\n' "================================================================" >&2
	printf '%s\n' "  LOOM COMMIT REMINDER (advisory)" >&2
	printf '%s\n' "================================================================" >&2
	printf '%s\n' "" >&2
	# Print reason with newline interpretation
	printf '%b\n' "$reason" >&2
	printf '%s\n' "" >&2
	printf '%s\n' "================================================================" >&2

	exit 0
}

# Whether this contract writer's latest `loom stage contracts freeze` failed.
# The CLI runs in the sandbox and cannot write the state directory, so it
# leaves the problems in the session's relay scratch directory and removes
# them once an attempt reaches the daemon (verify/contracts/refusal.rs,
# REFUSAL_FILE). A symlink there is never taken for a refusal.
contract_freeze_refused() {
	local refusal="${LOOM_SCRATCH_DIR:-}/contract-freeze-refused.txt"
	[[ -n "${LOOM_SCRATCH_DIR:-}" && -f "$refusal" && ! -L "$refusal" ]]
}

# Contract-phase reminder, printed in place of the commit-and-complete
# checklist, then allow stop. The hook calls no loom command for a contract
# session: the Stop hook fires at every turn end, background subagents
# running included, and the daemon files a standing refusal for adjudication
# once the session is quiet.
# Args: $1 = stage ID
remind_contract_freeze() {
	local stage_id="$1"

	printf '\n' >&2
	printf '%s\n' "================================================================" >&2
	printf '%s\n' "  LOOM CONTRACT REMINDER (advisory)" >&2
	printf '%s\n' "================================================================" >&2
	printf '%s\n' "Contract session for stage '$stage_id':" >&2
	if contract_freeze_refused; then
		printf '%s\n' "  - the latest freeze was refused: fix what it names and run" >&2
		printf '%s\n' "    loom stage contracts freeze $stage_id again, or stop; once this" >&2
		printf '%s\n' "    session is quiet for 5 minutes with no subagent running, the" >&2
		printf '%s\n' "    refusal goes to an adjudicator and a fresh writer starts with" >&2
		printf '%s\n' "    the verdict" >&2
	else
		printf '%s\n' "  - finish with: loom stage contracts freeze $stage_id" >&2
	fi
	printf '%s\n' "  - do not commit" >&2
	printf '%s\n' "  - do not complete the stage" >&2
	printf '%s\n' "After the freeze request, stop; loom ends this session." >&2
	printf '%s\n' "================================================================" >&2

	exit 0
}

# Non-blocking reminder about memory recording
# Outputs to stderr
remind_memory_usage() {
	local project_root="$1"
	local stage_id="$2"

	# Only this stage's own journal counts; another stage's entries do not.
	case "$stage_id" in
	"" | */*) return ;;
	esac
	local memory_dir="$project_root/$WORK_DIR/memory"
	if [[ ! -d "$memory_dir" ]]; then
		return
	fi
	local journal="$memory_dir/$stage_id.md"

	# More than 10 lines = real entries
	local has_entries=false
	if [[ -f "$journal" ]]; then
		local lines
		lines=$(wc -l <"$journal" | tr -d '[:space:]')
		if [[ "$lines" -gt 10 ]]; then
			has_entries=true
		fi
	fi

	# Show reminder if no substantial entries found
	if [[ "$has_entries" == "false" ]]; then
		printf '\n' >&2
		printf '%s\n' "------------------------------------------------------------" >&2
		printf '%s\n' "⚠️  Memory Recording Reminder" >&2
		printf '%s\n' "------------------------------------------------------------" >&2
		printf '%s\n' "No memory entries recorded for this stage." >&2
		printf '%s\n' "Did you make decisions or discoveries worth recording?" >&2
		printf '%s\n' "A knowledge-distill stage records nothing once distilling begins." >&2
		printf '%s\n' "" >&2
		printf '%s\n' "  loom memory decision \"choice\" --context \"why\"" >&2
		printf '%s\n' "  loom memory note \"observation\"" >&2
		printf '%s\n' "  loom memory show --all  # Reviewed by the knowledge-distill stage" >&2
		printf '%s\n' "------------------------------------------------------------" >&2
	fi
}

# Main hook logic
main() {
	debug_log "=== loom-stop hook starting ==="
	debug_log "CWD: $(pwd)"
	debug_log "LOOM_HOOK_DEBUG: ${LOOM_HOOK_DEBUG:-0}"

	# Merge resolution sessions are exempt from commit requirements
	# They resolve merge conflicts in the stage worktree; the stop hook does not require a commit
	if [ "${LOOM_MERGE_SESSION:-}" = "1" ]; then
		debug_log "Merge session detected (LOOM_MERGE_SESSION=1) - allowing stop"
		exit 0
	fi

	# Contract sessions write a stage's failing contract tests and hand over
	# through the freeze; committing or completing is the stage session's job
	if [ "${LOOM_SESSION_TYPE:-}" = "contract" ]; then
		remind_contract_freeze "${LOOM_STAGE_ID:-<stage-id>}"
	fi

    local worktree STAGE_ID work_path project_root
    worktree=$(loom_current_worktree) || exit 0
    worktree=${worktree%/}
    # The checklist reads git status: a session that moved out of its worktree
    # still has its stage's tree to account for, not the one it moved to.
    cd "$worktree" 2>/dev/null || exit 0
    STAGE_ID=${worktree##*/}
    work_path=$(loom_stage_work_dir "$worktree") || exit 0
    case "$work_path" in
        */.loom/work) project_root=${work_path%/.loom/work}; WORK_DIR=.loom/work ;;
        */.work) project_root=${work_path%/.work}; WORK_DIR=.work ;;
    esac

	# Log complete state for debugging
	debug_log "=== State Summary ==="
	debug_log "  Stage ID: $STAGE_ID"
	debug_log "  Project Root: $project_root"
	if [[ -L "$project_root/$WORK_DIR" ]]; then
		debug_log "  $WORK_DIR: symlink -> $(readlink -f "$project_root/$WORK_DIR" 2>/dev/null || echo 'unresolvable')"
		if [[ -e "$project_root/$WORK_DIR" ]]; then
			debug_log "  $WORK_DIR accessible: yes"
		else
			debug_log "  $WORK_DIR accessible: NO (broken symlink)"
		fi
	elif [[ -d "$project_root/$WORK_DIR" ]]; then
		debug_log "  $WORK_DIR: directory (not symlink)"
	else
		debug_log "  $WORK_DIR: does not exist"
	fi
	debug_log "===================="

	# Show memory reminder (non-blocking, informational)
	remind_memory_usage "$project_root" "$STAGE_ID"

	# Collect issues
	local issues=()
	local has_uncommitted=0
	local stage_incomplete=0
	local git_is_clean=0

	# Check for uncommitted changes
	if check_git_clean; then
		git_is_clean=1
	else
		has_uncommitted=1
		issues+=("UNCOMMITTED CHANGES detected")
	fi

	# Check stage status
	local stage_file status=""
	stage_file=$(loom_find_stage_file "$work_path" "$STAGE_ID" || true)

	if [[ -n "$stage_file" ]]; then
		status=$(loom_stage_status "$stage_file" || true)
		debug_log "Stage '$STAGE_ID' status: $status"

        if [[ "$status" == executing || "$status" == Executing ]]; then
            stage_incomplete=1
            issues+=("Stage '$STAGE_ID' is still in EXECUTING status")
            debug_log "Stage is still executing - showing reminder"
        fi
	else
		# Edge case: stage file not found
		# Missing or ambiguous stage metadata does not establish stage status.
		# If git is also clean, this is likely a non-loom worktree - allow stop
		debug_log "No stage file found for '$STAGE_ID'"
		if [[ $git_is_clean -eq 1 ]]; then
			debug_log "Git is clean and no stage file - likely non-loom worktree, allowing stop"
			exit 0
		fi
		debug_log "Git has uncommitted changes - will require commit even without stage file"
	fi

	# If no issues, allow stop
	if [[ ${#issues[@]} -eq 0 ]]; then
		debug_log "No issues found, allowing stop"
		exit 0
	fi

	debug_log "Found ${#issues[@]} issue(s), showing advisory"

	# Build error message with detailed context
	local message="LOOM WORKTREE COMPLETION CHECKLIST for stage '$STAGE_ID' (advisory):"
	message+="\n\nChecked: worktree=$(pwd), project_root=$project_root"

	if [[ $has_uncommitted -eq 1 ]]; then
		message+="\n\n1. You have uncommitted changes. Commit with git add <specific-files> then loom stage commit $STAGE_ID -m 'type(scope): description'; wait with loom request status <id> --wait 90; never run git commit."
		message+="\n   (NOTE: Do NOT use 'git add -A' or 'git add .' as these will stage $WORK_DIR)"
		local changes
		changes=$(get_uncommitted_changes)
		if [[ -n "$changes" ]]; then
			message+="\n\n   Modified files:\n"
			while IFS= read -r line; do
				message+="\n   $line"
			done <<<"$changes"
		fi
	fi

	if [[ $stage_incomplete -eq 1 ]]; then
		local step_num=1
		if [[ $has_uncommitted -eq 1 ]]; then
			step_num=2
		fi

		message+="\n\n${step_num}. Stage is still in '$status' status. Once the stage is SETTLED and committed, run:\n   loom stage complete $STAGE_ID"

		if [[ -n "$stage_file" ]]; then
			message+="\n   (Stage file: $stage_file)"
		fi
	fi

	message+="\n\nIMPORTANT: this is a checklist, NOT an instruction to complete right now."
	message+="\nThis reminder can fire while subagents are still running. If subagents are still"
	message+="\nout, results unabsorbed, or defects unfixed, KEEP WORKING: absorb every subagent"
	message+="\nresult, fix every defect, commit - and only THEN run loom stage complete."
	message+="\nloom stage complete is the LAST act of the session. Complete only a SETTLED stage,"
	message+="\nand run NOTHING after it - post-completion work is lost work."
	message+="\nRead its first line, Host job: <state>. requested, pending and stopped (exit 4):"
	message+="\nrun it again. failed, rejected and refused: fix, commit, run it again."
	message+="\ncompleted: end the session."

	message+="\n\nDo NOT end this session until all steps are complete, or a dispute or block is filed."

	# Add debug info section
	message+="\n\n--- Debug Info ---"
	message+="\nStage ID: $STAGE_ID"
	if [[ -n "$stage_file" ]]; then
		message+="\nParsed status: '$status'"
		message+="\nStage file: $stage_file"
	else
		message+="\nStage file: NOT FOUND"
	fi
	message+="\nTo enable verbose logging, set: LOOM_HOOK_DEBUG=1"
	message+="\n------------------"

	warn_with_reason "$message"
}

# Run main
main "$@"
