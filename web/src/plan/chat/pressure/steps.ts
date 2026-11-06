import type {
  PressureEnd,
  PressureRunView,
  PressureStepKind,
  PressureStepView,
} from "@/plan/api/schema-pressure";

export function stepLabel(kind: PressureStepKind): string {
  const labels: Record<PressureStepKind, string> = {
    claude_pressure: "/pressure",
    codex: "Codex",
    claude_address: "/address",
  };
  return labels[kind];
}

export function stepForSession(
  run: PressureRunView | null,
  sessionId: string,
): PressureStepView | null {
  return run?.steps.find((step) => step.session_id === sessionId) ?? null;
}

export function codexRunsAlone(run: PressureRunView | null): boolean {
  // The server lists a running codex step beside the Claude /pressure step; only run.step
  // switches to "codex" once the Claude half of the round has finished.
  return run !== null && run.ended === null && run.step === "codex";
}

// The wire end has no timestamp: use the last step that ended, else the moment the end arrived.
export function runEndMs(run: PressureRunView, arrivedMs: number): number {
  return run.steps.findLast((step) => step.ended_ms !== null)?.ended_ms ?? arrivedMs;
}

export function endLine(end: PressureEnd): string {
  switch (end.kind) {
    case "recorded":
      return `Pressure test recorded${end.line ? ` · ${end.line}` : ""}`;
    case "stopped":
      return `Stopped by ${end.by ?? ""}`;
    case "failed":
      return `Step ${end.step ?? ""} failed: ${end.reason ?? ""}`;
    case "interrupted":
      return "Interrupted";
  }
}

/** Every round runs /pressure, Codex and /address once. */
const STEPS_PER_ROUND = 3;

/** The plan's file name, the way a run names its plan. */
export function planFileName(plan: string): string {
  return plan.split("/").at(-1) ?? plan;
}

/** Counted work: the steps that stopped running out of every step the rounds will run. */
export function pressureProgress(run: PressureRunView): { done: number; total: number } {
  const total = Math.max(1, run.rounds * STEPS_PER_ROUND);
  const done = Math.min(total, run.steps.filter((step) => step.state !== "running").length);
  return { done, total };
}
