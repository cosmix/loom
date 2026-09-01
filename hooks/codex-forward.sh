#!/usr/bin/env bash
# codex-forward.sh - trusted argv boundary for the codex companion runtime

set -euo pipefail

if [[ $# -ne 7 || "$1" != "task" || "$3" != "--model" || "$5" != "--effort" || "$7" != "--write" ]]; then
	printf '%s\n' \
		'Usage: codex-forward.sh task <prompt> --model <model> --effort <effort> --write' >&2
	exit 2
fi

prompt=$2
model=$4
effort=$6

case "$model" in
gpt-5.6-sol | gpt-5.6-terra | gpt-5.6-luna) ;;
*)
	printf 'Unsupported forwarding model: %s\n' "$model" >&2
	exit 2
	;;
esac

case "$effort" in
low | medium | high | xhigh | max | ultra) ;;
*)
	printf 'Unsupported reasoning effort: %s\n' "$effort" >&2
	exit 2
	;;
esac

# codex reads AGENTS.md, never CLAUDE.md, and loom ships no AGENTS.md - this wrapper
# is the only place the codex lane's doctrine cannot be forgotten by an orchestrator
# writing a prompt, so it is prepended here on every forwarded task rather than left
# to each caller to remember.
preamble=$(cat <<'CODEX_PREAMBLE'
=== LOOM CONTEXT (prepended automatically; your task follows the TASK marker) ===

You are implementing one slice of a loom-orchestrated stage inside a git worktree. That
worktree is your boundary. An orchestrator verifies and commits your work; you do neither.

NAVIGATE WITH THE SOURCE GRAPH INSTEAD OF PAGING FILES.
Loom keeps a tree-sitter index of this repository. Each command below answers in well under a
second, never writes inside your worktree, and works from any directory in the tree. Use them
first, and open a file only once one of them has told you which lines matter:

  loom map --find-all <symbol>      every definition of a name: path, line, kind
  loom map --outline <file>         the symbols in a file, with line ranges and signatures
  loom map --impact <symbol|path>   what reaches it, with path confidence
  loom knowledge context --query "<question>" --budget-tokens 1500
                                    ranked project knowledge plus matching source
  rg -n '<pattern>' <path>          literal text search
  sed -n '<first>,<last>p' <file>   the exact lines a lookup pointed you at

Two things to expect from these commands, neither of which is a failure. They may print
`warning: could not refresh ...` or `warning: failed to refresh the context cache ...`, because
the cache they try to refresh lives outside your sandbox - the command still answers from the
published index, so do not retry it and do not report it as a block. And that index reflects your
branch point, so it will not show edits you or another agent made during this session; read a file
directly when you have already changed it.

DO NOT read CLAUDE.md. It instructs a different agent; nothing in it is addressed to you.
DO NOT read doc/loom/knowledge/ file by file. It is a ~200k-token corpus, and
`loom knowledge context --query` is how you query it. Ask it a question; do not read the library.

WRITE ONLY THE FILES YOUR TASK ASSIGNS YOU. Everything else in the tree is read-only to you.
NEVER write anything under .work/ - it is a symlink to state shared with other running stages.
NEVER run git: not add, not commit, not checkout, not stash, not restore.
DO NOT VERIFY. No full build, no test suite, no linter, no formatter, no type-checker, and never
a repeated or looping check. At most ONE narrowly-scoped check over the files you changed, run
once; skip it if you are unsure. The orchestrator compiles, tests, lints, and fixes.

FINISH BY REPORTING: files changed, assumptions you made, anything you could not resolve.

=== TASK ===
CODEX_PREAMBLE
)

task="${preamble}

${prompt}"

versions_dir=${HOME:?HOME is required}/.claude/plugins/cache/openai-codex/codex
shopt -s nullglob
candidates=("$versions_dir"/*/scripts/codex-companion.mjs)
shopt -u nullglob

if [[ ${#candidates[@]} -eq 0 ]]; then
	printf 'codex-companion.mjs not found under %s\n' "$versions_dir" >&2
	exit 1
fi

companion=${candidates[0]}
for candidate in "${candidates[@]:1}"; do
	if [[ "$candidate" > "$companion" ]]; then
		companion=$candidate
	fi
done

if [[ ! -f "$companion" || -L "$companion" ]]; then
	printf 'Refusing unsafe companion path: %s\n' "$companion" >&2
	exit 1
fi

status=0
node "$companion" task "$task" --write --model "$model" --effort "$effort" || status=$?

printf '\n--- LOOM-CODEX-EVIDENCE ---\n'
printf 'exit: %s\n' "$status"

shopt -s nullglob
job_files=("${HOME}"/.claude/plugins/data/codex-openai-codex/state/*/jobs/*.json)
shopt -u nullglob

if [[ ${#job_files[@]} -eq 0 ]]; then
	printf 'jobs: none found\n'
else
	# newest-first, capped at 3, via -nt insertion - no ls|head parsing
	newest=()
	for f in "${job_files[@]}"; do
		inserted=false
		for i in "${!newest[@]}"; do
			if [[ "$f" -nt "${newest[$i]}" ]]; then
				newest=("${newest[@]:0:$i}" "$f" "${newest[@]:$i}")
				inserted=true
				break
			fi
		done
		if [[ "$inserted" == false ]]; then
			newest+=("$f")
		fi
		if [[ ${#newest[@]} -gt 3 ]]; then
			newest=("${newest[@]:0:3}")
		fi
	done
	printf '%s\n' "${newest[@]}"
fi

exit "$status"
