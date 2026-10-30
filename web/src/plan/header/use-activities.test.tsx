import { act, cleanup, renderHook } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createStore } from "jotai/vanilla";
import { afterEach, expect, it } from "vitest";

import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import type { SessionView } from "@/plan/api/schema-chat";
import type { PressureRunView, PressureStepView } from "@/plan/api/schema-pressure";
import { summaryPanelSchema } from "@/plan/api/schema-summaries";
import { snapshotAtom } from "@/plan/chat/model";
import { pressureRunAtom } from "@/plan/chat/pressure/use-pressure";
import { runStripAtom } from "@/plan/run/use-run";
import { panelLoadAtom } from "@/plan/summaries/state";
import { ACTIVITY_LABELS } from "@/plan/ui/activity-glyphs";
import {
  deriveActivities,
  mostUrgent,
  useActivities,
  type ActivitiesInput,
} from "./use-activities";

afterEach(cleanup);

const startedAt = "2026-10-29T08:00:00Z";
const startedMs = Date.parse(startedAt);
const rebuild = {
  session: "summary-session",
  ids: ["a", "b", "c"],
  done: 1,
  total: 3,
  started_at: startedAt,
};
const empty: ActivitiesInput = {
  summaryPanel: null,
  runState: null,
  runStrip: null,
  pressureRun: null,
  session: null,
  pendingPermissions: [],
};

function session(state: SessionView["state"]): SessionView {
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
    subagents_running: 0,
    last_activity: null,
    failure: null,
    offer: null,
  };
}

function codexStep(round: number, since: number): PressureStepView {
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

function pressure(steps: PressureStepView[] = []): PressureRunView {
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

it("orders activities by urgency, then oldest start within a kind", () => {
  const activities = deriveActivities({
    ...empty,
    summaryPanel: { running: rebuild },
    runStrip: { daemon_running: true, plan: "PLAN-example.md" },
    pressureRun: pressure([codexStep(2, startedMs + 100), codexStep(1, startedMs)]),
    session: session("permission"),
    pendingPermissions: [{ tool: "Write" }],
  });
  expect(activities.map((item) => item.kind)).toEqual([
    "permission",
    "running",
    "pressure",
    "codex",
    "codex",
    "summaries",
  ]);
  expect(activities.filter((item) => item.kind === "codex").map((item) => item.since)).toEqual([
    startedMs,
    startedMs + 100,
  ]);
  expect(activities.map((item) => item.title)).toEqual([
    "Waiting for your answer",
    "Plan running",
    "Pressure test · round 1 of 2",
    ACTIVITY_LABELS.codex,
    ACTIVITY_LABELS.codex,
    "3 summaries rebuilding",
  ]);
  expect(mostUrgent([...activities].reverse())).toBe(activities[0]);
});

it("keeps operation ids stable as progress and permission details change", () => {
  const input: ActivitiesInput = {
    ...empty,
    summaryPanel: { running: rebuild },
    runStrip: { daemon_running: true, plan: "PLAN-example.md" },
    pressureRun: pressure([codexStep(1, startedMs)]),
    session: session("permission"),
    pendingPermissions: [{ tool: "Write" }],
  };
  const before = deriveActivities(input);
  const after = deriveActivities({
    ...input,
    summaryPanel: { running: { ...rebuild, done: 2 } },
    pressureRun: { ...pressure([codexStep(1, startedMs)]), tokens_total: 100 },
    pendingPermissions: [{ tool: "Edit" }, { tool: "Write" }],
  });
  expect(after.map((item) => item.id)).toEqual(before.map((item) => item.id));
  expect(before.map((item) => item.id)).toEqual([
    "permission:chat-1",
    "run",
    `pressure:doc/plans/PLAN-example.md:${startedMs}`,
    `codex:doc/plans/PLAN-example.md:${startedMs}:1`,
    "summaries",
  ]);
});

it("shows summary progress and its actual start time", () => {
  expect(deriveActivities({ ...empty, summaryPanel: { running: rebuild } })).toEqual([
    {
      id: "summaries",
      kind: "summaries",
      title: "3 summaries rebuilding",
      subject: "1 of 3",
      since: startedMs,
    },
  ]);
});

it("names the plan run with its merge progress, by file name", () => {
  expect(
    deriveActivities({
      ...empty,
      runStrip: { daemon_running: true, plan: "doc/plans/PLAN-example.md", merged: 1, total: 3 },
    }),
  ).toMatchObject([
    { kind: "running", title: "Plan running", subject: "PLAN-example.md · 1 of 3 stages merged" },
  ]);
  expect(
    deriveActivities({
      ...empty,
      runStrip: { daemon_running: true, plan: "doc/plans/PLAN-example.md", merged: 0, total: 0 },
    }),
  ).toMatchObject([{ subject: "PLAN-example.md" }]);
});

it("uses the singular for a one-summary rebuild", () => {
  const [row] = deriveActivities({
    ...empty,
    summaryPanel: { running: { ...rebuild, ids: ["a"], total: 1 } },
  });
  expect(row?.title).toBe("1 summary rebuilding");
});

it("returns no activities for empty state and no most urgent activity", () => {
  expect(deriveActivities(empty)).toEqual([]);
  expect(mostUrgent([])).toBeNull();
});

it.each([
  ["starting", "starting"],
  ["idle", "waiting"],
  ["ending", "drafting"],
  ["permission", "permission"],
] as const)("derives %s sessions from existing state", (state, kind) => {
  expect(deriveActivities({ ...empty, session: session(state) })).toEqual([
    {
      id: `${kind}:chat-1`,
      kind,
      title: kind === "permission" ? "Waiting for your answer" : ACTIVITY_LABELS[kind],
      subject: "Planning",
      since: startedMs,
    },
  ]);
});

it("gives pending permission requests precedence over the session state", () => {
  expect(
    deriveActivities({
      ...empty,
      session: session("idle"),
      pendingPermissions: [{ tool: "Write" }, { tool: "Bash" }],
    }),
  ).toMatchObject([
    {
      id: "permission:chat-1",
      kind: "permission",
      subject: "Write, Bash",
      since: null,
    },
  ]);
});

it.each(["working", "ended", "failed"] as const)(
  "does not infer document drafting from a %s session",
  (state) => {
    expect(deriveActivities({ ...empty, session: session(state) })).toEqual([]);
  },
);

it("does not show automatic pressure sessions as waiting or drafting", () => {
  for (const state of ["idle", "ending"] as const) {
    expect(
      deriveActivities({
        ...empty,
        session: { ...session(state), kind: "pressure" },
      }),
    ).toEqual([]);
  }
});

it("drops stale permissions when a session has ended", () => {
  expect(
    deriveActivities({
      ...empty,
      session: session("ended"),
      pendingPermissions: [{ tool: "Write" }],
    }),
  ).toEqual([]);
});

it("uses null for unknown timestamps and sorts them after known starts", () => {
  const unknown = deriveActivities({
    ...empty,
    session: { ...session("starting"), state_since: "unknown" },
  })[0]!;
  const known = deriveActivities({ ...empty, session: session("starting") })[0]!;
  expect(unknown.since).toBeNull();
  expect(mostUrgent([unknown, known])).toBe(known);
  expect(
    deriveActivities({
      ...empty,
      summaryPanel: { running: { ...rebuild, started_at: "unknown" } },
    })[0]?.since,
  ).toBeNull();
});

it("uses workspace state until a run strip arrives, then trusts daemon status", () => {
  const input: ActivitiesInput = {
    ...empty,
    runState: { workspace: "running", active_plan: "PLAN-example.md" },
  };
  expect(deriveActivities(input)).toMatchObject([
    {
      id: "run",
      kind: "running",
      subject: "PLAN-example.md",
      since: null,
    },
  ]);
  expect(
    deriveActivities({
      ...input,
      runStrip: { daemon_running: false, plan: "PLAN-example.md" },
    }),
  ).toEqual([]);
  expect(
    deriveActivities({
      ...empty,
      runState: { workspace: "stopped", active_plan: "PLAN-example.md" },
    }),
  ).toEqual([]);
});

it("reports concurrent Codex work and removes completed reviews and pressure runs", () => {
  const step = codexStep(1, startedMs);
  const run = pressure([step]);
  expect(deriveActivities({ ...empty, pressureRun: run }).map((item) => item.kind)).toEqual([
    "pressure",
    "codex",
  ]);
  expect(
    deriveActivities({
      ...empty,
      pressureRun: { ...run, steps: [{ ...step, state: "done" }] },
    }).map((item) => item.kind),
  ).toEqual(["pressure"]);
  expect(
    deriveActivities({
      ...empty,
      pressureRun: {
        ...run,
        steps: [
          {
            ...step,
            codex: {
              model: "codex",
              state: "review_written",
              report_path: "review.md",
              log_tail: null,
            },
          },
        ],
      },
    }).map((item) => item.kind),
  ).toEqual(["pressure"]);
  expect(
    deriveActivities({
      ...empty,
      pressureRun: {
        ...run,
        ended: {
          kind: "recorded",
          by: null,
          step: null,
          reason: null,
          line: null,
        },
      },
    }),
  ).toEqual([]);
});

it("reads existing atoms from a Jotai store and reacts when operations end", () => {
  const store = createStore();
  store.set(panelLoadAtom, {
    status: "ready",
    panel: summaryPanelSchema.parse({ ...panelFixture, running: rebuild }),
  });
  store.set(runStripAtom, {
    plan: "PLAN-example.md",
    daemon_running: true,
    merged: 0,
    total: 2,
    attention: 0,
    dashboard_url: null,
  });
  store.set(pressureRunAtom, pressure([codexStep(1, startedMs)]));
  store.set(snapshotAtom, { session: session("idle"), turns: [], queue: [], previous: null });
  const { result } = renderHook(useActivities, {
    wrapper: ({ children }) => <Provider store={store}>{children}</Provider>,
  });
  expect(result.current.map((item) => item.kind)).toEqual([
    "waiting",
    "running",
    "pressure",
    "codex",
    "summaries",
  ]);
  act(() => {
    store.set(panelLoadAtom, {
      status: "ready",
      panel: summaryPanelSchema.parse({ ...panelFixture, running: null }),
    });
    store.set(runStripAtom, null);
    store.set(pressureRunAtom, null);
    store.set(snapshotAtom, { session: null, turns: [], queue: [], previous: null });
  });
  expect(result.current).toEqual([]);
});
