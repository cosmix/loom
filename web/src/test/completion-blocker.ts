import type { CompletionBlocker, CompletionBlockerState } from "@/api/schema";

/// A completion blocker as `loom status` reports one for an executing stage
/// whose `loom stage complete` failed the test-integrity gate once.
export function completionBlocker(state: CompletionBlockerState = "pending"): CompletionBlocker {
  return {
    state,
    fingerprint: "feedface1234",
    failure_code: "daemon_rejected",
    summary: "test-integrity gate failed",
    commit: "0123456789ab",
    repeat_count: 1,
    first_observed_at: null,
    last_observed_at: null,
    next_action: "watching: a repeat of daemon_rejected parks this stage",
  };
}
