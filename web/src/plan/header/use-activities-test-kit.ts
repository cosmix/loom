import type { SessionView } from "@/plan/api/schema-chat";
import type { PressureRunView, PressureStepView } from "@/plan/api/schema-pressure";

/** Shared builders of `use-activities.test.tsx`. */
export const startedAt = "2026-10-29T08:00:00Z";
export const startedMs = Date.parse(startedAt);

export function session(state: SessionView["state"]): SessionView {
  return {
    id: "chat-1",
    kind: "authoring",
    state,
    name: "Planning",
    model: "claude",
    socket: "chat.sock",
    holder: null,
    away_until: null,
    queued: 0,
    started_at: startedAt,
    state_since: startedAt,
    working_since: null,
    fetch_grant: false,
    last_activity: null,
    failure: null,
    offer: null,
    feature: null,
    plan: null,
  };
}

export function codexStep(round: number, since: number): PressureStepView {
  return {
    round,
    kind: "codex",
    session_id: null,
    model: "codex",
    effort: "high",
    state: "running",
    started_ms: since,
    ended_ms: null,
    tokens: null,
    codex: { model: "codex", state: "running", report_path: null, log_tail: null },
    edits: [],
  };
}

export function pressure(steps: PressureStepView[] = []): PressureRunView {
  return {
    plan: "doc/plans/PLAN-example.md",
    rounds: 2,
    round: 1,
    step: "claude_pressure",
    started_ms: startedMs,
    tokens_total: 0,
    steps,
    ended: null,
  };
}
