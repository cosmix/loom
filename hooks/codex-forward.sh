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
gpt-5.6-terra | gpt-5.6-luna) ;;
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

exec node "$companion" task "$prompt" --write --model "$model" --effort "$effort"
