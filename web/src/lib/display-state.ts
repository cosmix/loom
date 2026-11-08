import type { StageStatus, StageSummary } from "@/api/schema";

/// The two states a completion blocker draws over an executing stage.
export type CompletionDisplayState = "completion-pending" | "completion-blocked";

/// What the dashboard draws for a stage: its wire status, or the completion
/// blocker that overrides it.
export type DisplayState = StageStatus | CompletionDisplayState;

/// The state a stage shows, by the rule `loom status` uses to list it under
/// "Requires Attention": an executing stage whose `loom stage complete` failed
/// a gate keeps its session and its `executing` status, but waits on that
/// failure being fixed or disputed, so it shows its blocker. A parked stage
/// keeps its own state, which already asks for the operator.
export function displayState(stage: StageSummary): DisplayState {
  const blocker = stage.completion_blocker?.state;
  if (stage.status !== "executing" || !blocker) return stage.status;
  return blocker === "pending" ? "completion-pending" : "completion-blocked";
}

export function isCompletionState(state: DisplayState | null): state is CompletionDisplayState {
  return state === "completion-pending" || state === "completion-blocked";
}
