import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { useState } from "react";

import { sessionViewSchema } from "@/plan/api/schema-chat";
import { pressureRunSchema } from "@/plan/api/schema-pressure";
import type { CodexStatus, PressureStepKind, PressureStepView } from "@/plan/api/schema-pressure";
import { pressureRunAtom } from "@/plan/chat/pressure/use-pressure";
import { RunHeader } from "@/plan/chat/run-header";
import type { PlannerHelpScene } from "@/plan/help/pinned-scene";
import { SAMPLE_HOLDER, SAMPLE_SERVER } from "@/plan/help/sample";
import { SceneRouter } from "@/plan/help/scene-router";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";

const PLAN = "doc/plans/PLAN-atlas.md";
const REPORT = "doc/plans/reviews/atlas-codex.md";
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
});
const CODEX: CodexStatus = {
  model: "gpt-6.1-sol",
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
  const store = createStore();
  store.set(serverInfoAtom, SAMPLE_SERVER);
  store.set(youAtom, SAMPLE_SERVER.you);
  store.set(authoringAtom, SAMPLE_HOLDER);
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

/** One finished round whose end was recorded, with the Codex report and the address edits. */
function recordedStore() {
  const codex: CodexStatus = { ...CODEX, state: "review_written", report_path: REPORT };
  return seededStore({
    plan: PLAN,
    rounds: 1,
    round: 1,
    step: "claude_address",
    started_ms: 1000,
    tokens_total: 12600,
    steps: [
      step("codex", { ended_ms: 181000, codex }, 1000),
      step("claude_pressure", { ended_ms: 181000 }, 1000),
      step(
        "claude_address",
        { ended_ms: 301000, edits: [PLAN, "doc/plans/briefs/atlas.md"] },
        181000,
      ),
    ],
    ended: { kind: "recorded", by: null, step: null, reason: null, line: "1 round complete" },
  });
}

export function PressureHeaderScene() {
  const [store] = useState(runningStore);
  return (
    <Provider store={store}>
      <SceneRouter>
        <div data-loom-plan inert className="max-w-[680px]">
          <RunHeader session={SESSION} />
        </div>
      </SceneRouter>
    </Provider>
  );
}

export function PressureRunScene() {
  const [store] = useState(recordedStore);
  return (
    <Provider store={store}>
      <SceneRouter>
        <div data-loom-plan inert className="max-w-[680px]">
          <RunHeader session={SESSION} />
        </div>
      </SceneRouter>
    </Provider>
  );
}

// The run strip is not shown: RunStrip mounts DashboardButton, which POSTs /api/run/dashboard on mount; revisit when the fetch can be injected.
export const PRESSURE_SCENES: readonly PlannerHelpScene[] = [
  {
    title: "The run header",
    Scene: PressureHeaderScene,
    notes: [
      {
        target: ".chat-run-header",
        text: "The header line names the plan, the round, the active step, the elapsed time and the token total.",
      },
      {
        target: '[aria-label="Round 1 Codex"]',
        text: "The Codex card shows the background review model and how long it has been running.",
      },
      {
        target: '[aria-label="Round 1 /pressure"]',
        text: "The /pressure card runs beside Codex in the same round and reports its own token total.",
      },
      {
        target: "[data-session-id] button",
        text: "Stop asks for confirmation, then ends the pressure test and keeps the edits made so far.",
      },
    ],
  },
  {
    title: "Following a run",
    Scene: PressureRunScene,
    notes: [
      {
        target: '[aria-label="Round 1 Codex"] a',
        text: "Review written opens the report that Codex produced.",
      },
      {
        target: '[aria-label="Round 1 /address"]',
        text: "The address card lists the planning documents edited after the reviews.",
      },
      {
        target: '[aria-label="Round 1 /address"] [title^="Input:"]',
        text: "Hover a step's token total to see its input, cache and output counts.",
      },
      {
        target: '[aria-label="Edited files"]',
        text: "The summary lists every edited document with a link to its diff.",
      },
      {
        target: '[aria-label="Pressure test summary"] > div:last-child > a',
        text: "Review the changes opens the plan diff once the pressure test is recorded.",
      },
    ],
  },
];

export const PRESSURE_TIPS: readonly string[] = [
  "Each pressure round runs two reviews in parallel before an address step edits the plan and its briefs.",
  "Starting a pressure test ends an open authoring chat with its closing write.",
];
