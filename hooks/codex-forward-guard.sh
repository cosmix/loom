#!/usr/bin/env bash
# codex-forward-guard.sh - PreToolUse hook pinning codex forwarders to forwarding
#
# The codex lane spawns a wrapper subagent (loom-codex-forwarder) whose ONLY job
# is one Bash call handing the task to codex-companion.mjs. A wrapper with a
# full toolset can instead implement the task itself - observed 2026-08-07: a
# wrapper received a codex prompt and made 26 Edit calls directly, silently
# replacing the gpt-5.6-luna lane with unreviewed sonnet output. Two gaps made
# that possible: PLUGIN agents' `tools:` frontmatter is ignored BY DESIGN
# (code.claude.com/docs/en/sub-agents#available-tools), so the plugin wrapper
# ran with a full toolset; and `loom_is_subagent` (process-tree based) returns
# false for in-process subagents, so the existing guards never engaged. The
# loom-owned forwarder's `tools: Bash` IS hard-enforced (user-scope agent);
# this hook backstops that against a Bash-shaped escape (sed -i, tee, git)
# and pins a direct plugin spawn, which has no tool restriction at all.
#
# DETECTION, two layers, payload-based not process-based:
#   1. PRIMARY - agent_type. When a hook fires inside a subagent the payload
#      carries `agent_type` (and `agent_id`) per the hooks documentation.
#      "loom-codex-forwarder" and the plugin's "codex:codex-rescue" are both
#      forwarding shims - enforce for either, so even a direct plugin spawn
#      (which doctrine forbids) is still pinned to forwarding.
#   2. FALLBACK - sentinel. On harness builds that do not set agent_type, a
#      transcript_path matching */subagents/agent-*.jsonl whose opening bytes
#      carry the LOOM-CODEX-FORWARD-ONLY sentinel identifies a codex-lane
#      subagent (signal doctrine puts the sentinel on the FIRST LINE of every
#      codex prompt and forbids it in any other lane's prompt). A main
#      session's transcript never matches the path shape, so the sentinel
#      embedded in the orchestrator's own Agent tool_use cannot self-block.
# The sentinel literal must agree with CODEX_FORWARD_SENTINEL in
# loom/src/codex.rs - tests_doctrine.rs pins the two together.
#
# FAIL-OPEN by design: no agent_type, no transcript_path, an unreadable file,
# or a missing sentinel all allow the call. On a harness exposing neither
# field this hook is a no-op and the doctrinal layers (agent definition,
# signal prose, orchestrator evidence check) still apply - it only ever
# raises the cost, mirroring subagent-verify-guard's philosophy.
#
# Input: JSON from stdin - {"tool_name": ..., "tool_input": ...,
#        "agent_type": ..., "transcript_path": ...}
# Exit codes: 0 = allow, 2 = block with guidance on stderr

set -euo pipefail

source "$(dirname "$0")/_common.sh"

# Read stdin under gtimeout (macOS+coreutils), timeout (Linux), or bare cat
if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
	INPUT_JSON=$(cat 2>/dev/null || true)
fi

TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
AGENT_TYPE=$(echo "$INPUT_JSON" | jq -r '.agent_type // empty' 2>/dev/null || true)
TRANSCRIPT_PATH=$(echo "$INPUT_JSON" | jq -r '.transcript_path // empty' 2>/dev/null || true)

[[ -n "$TOOL_NAME" ]] || exit 0

# enforce - allow only the single codex-companion.mjs Bash call; block the rest.
enforce() {
	if [[ "$TOOL_NAME" == "Bash" ]]; then
		local command
		command=$(echo "$INPUT_JSON" | jq -r '.tool_input.command // empty' 2>/dev/null || true)
		case "$command" in
		*codex-companion.mjs*) exit 0 ;;
		esac
	fi
	loom_debug "DEBUG: BLOCKED codex forwarder tool=$TOOL_NAME agent_type=$AGENT_TYPE transcript=$TRANSCRIPT_PATH"
	cat >&2 <<'EOF'
⛔ BLOCKED: You are a codex FORWARDING SHIM (your prompt carries LOOM-CODEX-FORWARD-ONLY).

FORWARD, DO NOT IMPLEMENT:
- Your ONLY permitted tool call is ONE Bash call invoking codex-companion.mjs task ...
- Do NOT read files, search the repo, edit anything, or implement any part of the
  task yourself. The task was routed to the codex lane so Codex writes the code.
- If your forward attempt failed, return the complete error output verbatim,
  prefixed LOOM-CODEX-FORWARD-ERROR, and stop. A failed forward is a reportable
  failure, not a license to implement.
EOF
	exit 2
}

# === PRIMARY GATE: the calling agent IS a forwarding shim by type ===
case "$AGENT_TYPE" in
loom-codex-forwarder | codex:codex-rescue) enforce ;;
esac

# === FALLBACK GATE: sentinel in the subagent's own transcript ===
[[ -n "$TRANSCRIPT_PATH" ]] || exit 0

# Only a subagent's OWN transcript qualifies. The main session's transcript
# also contains the sentinel (inside the Agent tool_use that spawned the
# forwarder), so matching on content alone would throttle the orchestrator.
case "$TRANSCRIPT_PATH" in
*/subagents/agent-*.jsonl) ;;
*) exit 0 ;;
esac

[[ -r "$TRANSCRIPT_PATH" ]] || exit 0

# The prompt is in the transcript's opening bytes; 200KB covers any signal-
# sized prompt without paying to scan a long transcript on every tool call.
head -c 200000 "$TRANSCRIPT_PATH" 2>/dev/null | grep -qF 'LOOM-CODEX-FORWARD-ONLY' || exit 0

enforce
