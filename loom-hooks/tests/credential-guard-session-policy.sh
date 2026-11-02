#!/usr/bin/env bash
set -euo pipefail
HOOK="$(cd "$(dirname "$0")/.." && pwd)/credential-guard.sh"
ROOT=$(mktemp -d)
trap 'rm -rf "$ROOT"' EXIT
mkdir -p "$ROOT/checkout/secrets" "$ROOT/state/capsules" "$ROOT/home"
printf 'secret\n' >"$ROOT/checkout/secrets/key"
printf 'safe\n' >"$ROOT/checkout/secrets/neighbor"
printf 'token\n' >"$ROOT/state/admin.token"
printf 'token\n' >"$ROOT/state/user.token"
printf 'key\n' >"$ROOT/state/completion-attestation.key"
ln -s "$ROOT/state" "$ROOT/checkout/state-link"
printf '{"sandbox":{"filesystem":{"denyRead":["secrets/key"]}}}\n' >"$ROOT/state/capsules/session-b.settings.json"

check() {
	local expected="$1" tool="$2" path="$3" stage="${4:-knowledge}" status=0 payload
	case "$tool" in
	Grep | Glob) payload=$(printf '{"tool_name":"%s","tool_input":{"path":"%s","pattern":"*"}}' "$tool" "$path") ;;
	*) payload=$(printf '{"tool_name":"%s","tool_input":{"file_path":"%s"}}' "$tool" "$path") ;;
	esac
	HOME="$ROOT/home" CLAUDE_PROJECT_DIR="$ROOT/checkout" LOOM_SESSION_ID=session-b \
		LOOM_STAGE_ID="$stage" LOOM_WORK_DIR="$ROOT/state" bash "$HOOK" <<<"$payload" >/dev/null 2>&1 || status=$?
	if [[ "$status" != "$expected" ]]; then
		printf 'FAIL: %s %s expected %s, got %s\n' "$tool" "$path" "$expected" "$status" >&2
		exit 1
	fi
}
for tool in Read Write Edit MultiEdit NotebookEdit Grep Glob; do
	check 2 "$tool" "$ROOT/checkout/secrets/key"
	check 0 "$tool" "$ROOT/checkout/secrets/neighbor"
done
# A checkout-rooted adjudication session reads the same capsule policy.
check 2 Read "$ROOT/checkout/secrets/key" adjudicate-stage
check 0 Read "$ROOT/checkout/secrets/neighbor" adjudicate-stage
for secret in admin.token user.token completion-attestation.key; do
	check 2 Read "$ROOT/checkout/state-link/$secret"
done
printf 'invalid\n' >"$ROOT/state/capsules/session-b.settings.json"
check 2 Read "$ROOT/checkout/secrets/neighbor"
rm "$ROOT/state/capsules/session-b.settings.json"
check 2 Read "$ROOT/checkout/secrets/neighbor"
printf 'PASS: session policy and aliased state root\n'
