import { expect, it } from "vitest";

import fixture from "@/plan/api/fixtures/pressure-run.json";
import {
  pressureRunSchema,
  type PressureEnd,
  type PressureStepKind,
  type PressureStepView,
} from "@/plan/api/schema-pressure";

import { codexRunsAlone, endLine, stepForSession, stepLabel } from "./steps";

const run = pressureRunSchema.parse(fixture);
const codex = run.steps[1]!;
const ended: PressureEnd = { kind: "recorded", by: null, step: null, reason: null, line: null };

it.each<[PressureStepKind, string]>([
  ["claude_pressure", "/pressure"],
  ["codex", "Codex"],
  ["claude_address", "/address"],
])("labels %s as %s", (kind, label) => {
  expect(stepLabel(kind)).toBe(label);
});

it("finds the step belonging to a Claude session", () => {
  expect(stepForSession(run, "session-1")).toBe(run.steps[0]);
});

it("finds a later session by ID rather than by the run's current step", () => {
  const address: PressureStepView = {
    ...codex,
    kind: "claude_address",
    session_id: "session-2",
    codex: null,
  };

  expect(stepForSession({ ...run, steps: [...run.steps, address] }, "session-2")).toBe(address);
});

it("returns null when there is no run or no matching session", () => {
  expect(stepForSession(null, "session-1")).toBeNull();
  expect(stepForSession(run, "missing")).toBeNull();
  expect(stepForSession({ ...run, steps: [] }, "session-1")).toBeNull();
});

it("reports Codex alone when the ongoing run's current step is codex", () => {
  expect(run.step).toBe("codex");
  expect(codexRunsAlone(run)).toBe(true);
});

it("does not report Codex alone while Claude /pressure runs beside a running Codex step", () => {
  const claude: PressureStepView = { ...run.steps[0]!, state: "running", ended_ms: null };

  expect(codexRunsAlone({ ...run, step: "claude_pressure", steps: [claude, codex] })).toBe(false);
});

it.each<PressureStepKind>(["claude_pressure", "claude_address"])(
  "does not report Codex alone when the run's current step is %s",
  (step) => {
    const latest: PressureStepView = { ...codex, kind: step, session_id: "next", codex: null };

    expect(codexRunsAlone({ ...run, step, steps: [...run.steps, latest] })).toBe(false);
  },
);

it.each<PressureEnd["kind"]>(["recorded", "stopped", "failed", "interrupted"])(
  "does not report Codex alone when the run ended as %s",
  (kind) => {
    expect(codexRunsAlone({ ...run, ended: { ...ended, kind } })).toBe(false);
  },
);

it("does not report Codex alone without a run", () => {
  expect(codexRunsAlone(null)).toBe(false);
});

it.each<[PressureEnd, string]>([
  [ended, "Pressure test recorded"],
  [{ ...ended, line: "" }, "Pressure test recorded"],
  [{ ...ended, line: "Round 2 completed" }, "Pressure test recorded · Round 2 completed"],
  [{ ...ended, kind: "stopped", by: "Dana" }, "Stopped by Dana"],
  [
    { ...ended, kind: "failed", step: "codex", reason: "Process exited" },
    "Step codex failed: Process exited",
  ],
  [{ ...ended, kind: "interrupted" }, "Interrupted"],
])("formats the end line for %j", (end, expected) => {
  expect(endLine(end)).toBe(expected);
});
