#!/usr/bin/env bash
# Exercise the shell's path fallback after loom returns no pressure candidates.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
LOOM_BIN=${LOOM_BIN:-$ROOT/loom/target/debug/loom}
[[ -x "$LOOM_BIN" ]] || { echo "SKIP: loom binary unavailable"; exit 0; }

TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-completion.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/bin" "$TMP/paths" "$TMP/home"
ln -s "$LOOM_BIN" "$TMP/bin/loom"
: >"$TMP/paths/pressure-only.md"
"$LOOM_BIN" completions bash >"$TMP/loom.bash"
"$LOOM_BIN" completions fish >"$TMP/loom.fish"
rg -Fq "complete -c loom -n '__fish_seen_subcommand_from init pressure' -F" "$TMP/loom.fish" || {
    echo "FAIL: generated fish script disables pressure path fallback"
    exit 1
}

if command -v expect >/dev/null 2>&1; then
    LOOM_COMPLETION_FIXTURE="$TMP" PATH="$TMP/bin:$PATH" expect <<'EXPECT'
log_user 0
set timeout 10
set fixture $env(LOOM_COMPLETION_FIXTURE)
set env(HOME) "$fixture/home"
set env(PS1) "ready> "
spawn bash --noprofile --norc -i
expect "ready> "
send "cd $fixture/paths\r"
expect "ready> "
send "source $fixture/loom.bash\r"
expect "ready> "
send "loom pressure pressure-o\t"
expect {
    "loom pressure pressure-only.md" {}
    timeout { puts stderr "FAIL: Bash did not complete the pressure path"; exit 1 }
}
send "\025exit\r"
expect eof
EXPECT
else
    echo "SKIP: expect unavailable; Bash runtime fallback unverified"
fi

if command -v fish >/dev/null 2>&1; then
    LOOM_COMPLETION_FISH_SCRIPT="$TMP/loom.fish" LOOM_COMPLETION_PATHS="$TMP/paths" \
        fish -c 'source "$LOOM_COMPLETION_FISH_SCRIPT"; cd "$LOOM_COMPLETION_PATHS"; complete -C "loom pressure pressure-o"' \
        >"$TMP/fish-candidates"
    if ! rg -q '^pressure-only\.md([[:space:]]|$)' "$TMP/fish-candidates"; then
        echo "FAIL: fish did not complete the pressure path"
        exit 1
    fi
else
    echo "SKIP: fish unavailable; native fish fallback unverified"
fi

echo "PASS"
