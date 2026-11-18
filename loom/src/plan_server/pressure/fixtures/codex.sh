#!/bin/sh
[ "$1" = exec ] || exit 1
d=$(dirname "$0")
model=$5
root=$9
plan=${10#\$pressure }
echo "$$ $model" >> "$d/pids"
echo "codex fake run with $model"
if [ -f "$d/unavailable-$model" ]; then
  sleep 0.3
  echo "ERROR: model not found: $model"
  exit 1
fi
if [ -f "$d/report" ]; then
  echo "# Review" > "$root/$(dirname "$plan")/codex-$(basename "$plan")"
fi
if [ -f "$d/config" ]; then
  mkdir -p "$root/.claude"
  echo '{}' > "$root/.claude/settings.local.json"
fi
if [ -f "$d/stubborn" ]; then
  trap '' TERM
  sleep "$(cat "$d/stubborn")" &
  child=$!
  echo "$child" > "$d/stubborn-child"
  wait "$child"
fi
if [ -f "$d/sleep" ]; then
  trap 'kill $child 2>/dev/null; exit 143' TERM
  sleep "$(cat "$d/sleep")" &
  child=$!
  wait $child
fi
echo "codex fake done"
exit "$(cat "$d/exit" 2>/dev/null || echo 0)"
