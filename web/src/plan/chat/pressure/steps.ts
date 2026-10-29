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
