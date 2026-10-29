import { describe, expect, it } from "vitest";
import options from "@/plan/api/fixtures/pressure-options.json";
import run from "@/plan/api/fixtures/pressure-run.json";
import { pressureOptionsSchema, pressureRunSchema } from "./schema-pressure";

describe("pressure wire schemas", () => {
  it("parses the options fixture and rejects extra fields", () => {
    expect(pressureOptionsSchema.parse(options)).toEqual(options);
    expect(pressureOptionsSchema.safeParse({ ...options, extra: true }).success).toBe(false);
    expect(
      pressureOptionsSchema.safeParse({ ...options, defaults: { ...options.defaults, extra: true } })
        .success,
    ).toBe(false);
  });

  it("parses the run fixture and rejects invalid nested values", () => {
    expect(pressureRunSchema.parse(run)).toEqual(run);
    expect(pressureRunSchema.safeParse({ ...run, step: "unknown" }).success).toBe(false);
    expect(pressureRunSchema.safeParse({ ...run, rounds: 4294967296 }).success).toBe(false);
    expect(pressureRunSchema.safeParse({ ...run, started_ms: -1 }).success).toBe(false);
    expect(
      pressureRunSchema.safeParse({ ...run, steps: [{ ...run.steps[0], extra: true }] }).success,
    ).toBe(false);
  });
});
