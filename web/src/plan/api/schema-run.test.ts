import { describe, expect, it } from "vitest";
import runOutput from "@/plan/api/fixtures/run-output.json";
import runState from "@/plan/api/fixtures/run-state.json";
import runStrip from "@/plan/api/fixtures/run-strip.json";
import { runOutputSchema, runStateSchema, runStripSchema } from "./schema-run";

describe("run wire schemas", () => {
  const fixtures = [
    ["state", runStateSchema, runState],
    ["strip", runStripSchema, runStrip],
    ["output", runOutputSchema, runOutput],
  ] as const;

  it.each(fixtures)("parses %s and rejects extra fields", (_name, schema, fixture) => {
    expect(schema.safeParse(fixture).success).toBe(true);
    expect(schema.safeParse({ ...fixture, extra: true }).success).toBe(false);
  });

  it("rejects invalid enum values and extra nested fields", () => {
    expect(runStateSchema.safeParse({ ...runState, offer: "unknown" }).success).toBe(false);
    expect(runOutputSchema.safeParse({ ...runOutput, stream: "unknown" }).success).toBe(false);
    expect(
      runStateSchema.safeParse({
        ...runState,
        abandon: {
          ...runState.abandon,
          unmerged_branches: [{ branch: "topic", ahead: 1, extra: true }],
        },
      }).success,
    ).toBe(false);
  });

  it("rejects numbers outside Rust integer ranges", () => {
    expect(runStripSchema.safeParse({ ...runStrip, merged: -1 }).success).toBe(false);
    expect(runStripSchema.safeParse({ ...runStrip, total: 4294967296 }).success).toBe(false);
    expect(runOutputSchema.safeParse({ ...runOutput, exit_code: 2147483648 }).success).toBe(false);
    expect(runOutputSchema.safeParse({ ...runOutput, exit_code: -2147483649 }).success).toBe(false);
  });
});
