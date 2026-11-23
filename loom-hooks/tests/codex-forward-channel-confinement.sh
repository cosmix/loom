#!/usr/bin/env bash
# Runtime proof of the background-output trust boundary, using real srt.
# A missing prerequisite, or an srt that cannot start a sandbox here (for
# instance inside another sandbox), skips with exit 0. Set
# LOOM_HOOK_TEST_REQUIRE_SRT=1 to turn that skip into a failure; no CI job sets
# it today, so a runner without srt skips this probe.
set -euo pipefail
# unavailable <reason> - skip, or fail when the runtime is required.
unavailable() {
    if [[ "${LOOM_HOOK_TEST_REQUIRE_SRT:-}" == 1 ]]; then
        echo "FAIL: $1" >&2
        exit 1
    fi
    echo "SKIP: $1"
    exit 0
}
if [[ "$(uname -s)" != Linux ]]; then
    echo 'SKIP: native task-output confinement probe requires Linux'
    exit 0
fi
runtime="${LOOM_TEST_SRT_BIN:-srt}"
for tool in "$runtime" bwrap socat python3; do
    command -v "$tool" >/dev/null || unavailable "missing runtime prerequisite: $tool"
done
d=$(mktemp -d)
root="/tmp/claude-$(id -u)"
mkdir -p "$root"
channel_dir=$(mktemp -d "$root/loom-channel-proof.XXXXXX")
trap 'rm -rf "$d" "$channel_dir"' EXIT
channel="$channel_dir/task.output"
printf original >"$channel"
python3 - "$d/settings.json" "$root" <<'PY'
import json, sys
with open(sys.argv[1], 'w') as target:
    json.dump({"network":{"allowedDomains":[],"deniedDomains":[]},
               "filesystem":{"denyRead":[],"allowRead":[],
                             "allowWrite":["/tmp"],"denyWrite":[sys.argv[2]]}}, target)
PY
"$runtime" --settings "$d/settings.json" -c true >/dev/null 2>&1 ||
    unavailable "srt cannot start a sandbox here"
cat >"$d/probe.py" <<'PY'
import os, pathlib, sys, tempfile
channel = pathlib.Path(sys.argv[1])
try:
    with channel.open('r+') as target:
        target.write('forged')
except OSError:
    pass
else:
    raise AssertionError('same-inode task output write was allowed')
with tempfile.NamedTemporaryFile() as scratch:
    scratch.write(b'ordinary temporary files remain writable')
print('HOST-STDOUT-REMAINS-WRITABLE', flush=True)
PY
"$runtime" --settings "$d/settings.json" -c "python3 '$d/probe.py' '$channel'" >"$channel" 2>"$d/stderr" || {
    cat "$d/stderr" >&2
    exit 1
}
[[ "$(cat "$channel")" == HOST-STDOUT-REMAINS-WRITABLE ]]
echo PASS
