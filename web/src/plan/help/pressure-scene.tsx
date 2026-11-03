import { Provider } from "jotai/react";
import { useState } from "react";

import { sessionViewSchema } from "@/plan/api/schema-chat";
import { pressureRunSchema } from "@/plan/api/schema-pressure";
import type { CodexStatus, PressureStepKind, PressureStepView } from "@/plan/api/schema-pressure";
import { pressureRunAtom } from "@/plan/chat/pressure/use-pressure";
import { RunHeader } from "@/plan/chat/run-header";
import type { PlannerHelpScene } from "@/plan/help/pinned-scene";
import { createSampleStore, PANEL_LOOK, SAMPLE_HOLDER } from "@/plan/help/sample";
import { SceneRouter } from "@/plan/help/scene-router";

const PLAN = "doc/plans/PLAN-atlas.md";
const SESSION = sessionViewSchema.parse({
  id: "pressure-help",
  kind: "pressure",
  state: "working",
  name: "Pressure test",
  model: "claude-sonnet",
  socket: "pressure-help.sock",
  holder: SAMPLE_HOLDER.holder.name,
  away_until: null,
  queued: 0,
  started_at: "2026-10-17T08:00:00Z",
  state_since: "2026-10-17T08:00:00Z",
  working_since: "2026-10-17T08:00:00Z",
  subagents_running: 0,
  last_activity: null,
  failure: null,
  offer: null,
  feature: null,
  plan: null,
});
const CODEX: CodexStatus = {
  model: "gpt-6.1-terra",
  state: "running",
  report_path: null,
  log_tail: null,
};
function step(
  kind: PressureStepKind,
  fields: Partial<PressureStepView>,
  startedMs: number,
): PressureStepView {
  return {
    round: 1,
    kind,
    session_id: null,
    model: kind === "codex" ? CODEX.model : "claude-sonnet",
    effort: "high",
    state: "done",
    started_ms: startedMs,
    ended_ms: null,
    tokens: { input: 2400, cache_write: 200, cache_read: 1000, output: 600 },
    codex: null,
    edits: [],
    ...fields,
  };
}

function seededStore(run: unknown) {
  const store = createSampleStore();
  store.set(pressureRunAtom, pressureRunSchema.parse(run));
  return store;
}

/** Round 1 of 2 in flight: Codex and /pressure review side by side, as the server runs them. */
function runningStore() {
  const now = Date.now();
  return seededStore({
    plan: PLAN,
    rounds: 2,
    round: 1,
    step: "claude_pressure",
    started_ms: now - 95000,
    tokens_total: 4200,
    steps: [
      step("codex", { state: "running", tokens: null, codex: CODEX }, now - 90000),
      step("claude_pressure", { state: "running" }, now - 90000),
    ],
    ended: null,
  });
}

export function PressureRunScene() {
  const [store] = useState(runningStore);
  return (
    <Provider store={store}>
      <SceneRouter>
        <div data-loom-plan inert className={PANEL_LOOK}>
          <RunHeader session={SESSION} />
        </div>
      </SceneRouter>
    </Provider>
  );
}

// The activity popover's run row is not shown: its RunRow mounts DashboardButton, which POSTs /api/run/dashboard on mount; revisit when the fetch can be injected.
export const PRESSURE_SCENES: readonly PlannerHelpScene[] = [
  {
    title: "A run in the chat",
    Scene: PressureRunScene,
    panel: true,
    notes: [
      {
        target: ".chat-run-header",
        text: "The plan, the round and the step. Stop ends the whole run.",
      },
      {
        target: '[aria-label="Round 1 Codex"]',
        text: "Codex reviews in the background; its report links here when written.",
      },
      {
        target: '[aria-label="Round 1 /pressure"]',
        text: "Claude reviews at the same time, then a second session addresses both reviews.",
      },
    ],
  },
];

export const PRESSURE_TIPS: readonly string[] = [
  "Each pressure round runs two reviews in parallel before an address step edits the plan and its briefs.",
  "A finished run summarizes the edited files and links to the diff of the changes.",
  "Starting a pressure test ends an open authoring chat with its closing write.",
];
