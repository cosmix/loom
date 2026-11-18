import { describe, expect, it } from "vitest";
import options from "@/plan/api/fixtures/pressure-options.json";
import run from "@/plan/api/fixtures/pressure-run.json";
import { planMessageSchema } from "./schema";
import { pressureOptionsSchema, pressureRunSchema } from "./schema-pressure";

describe("pressure wire schemas", () => {
  it("parses the options-changed frame as a bare tag, like server_stopping", () => {
    const frame = { type: "pressure_options_changed" };
    expect(planMessageSchema.parse(frame)).toEqual(frame);
    expect(planMessageSchema.safeParse({ ...frame, extra: true }).success).toBe(false);
  });

  it("parses the options fixture and rejects extra fields", () => {
    expect(pressureOptionsSchema.parse(options)).toEqual(options);
    expect(pressureOptionsSchema.safeParse({ ...options, extra: true }).success).toBe(false);
    expect(
      pressureOptionsSchema.safeParse({
        ...options,
        defaults: { ...options.defaults, extra: true },
      }).success,
    ).toBe(false);
  });

  it("requires config_block and parses a populated block", () => {
    const block = {
      reason: "Codex changed session configuration: AGENTS.md",
      changed: ["AGENTS.md"],
    };
    const { config_block: _omitted, ...without } = { ...options, config_block: null };
    expect(pressureOptionsSchema.safeParse(without).success).toBe(false);
    expect(pressureOptionsSchema.parse({ ...options, config_block: null }).config_block).toBeNull();
    expect(pressureOptionsSchema.parse({ ...options, config_block: block }).config_block).toEqual(
      block,
    );
    expect(
      pressureOptionsSchema.safeParse({ ...options, config_block: { ...block, extra: true } })
        .success,
    ).toBe(false);
    expect(
      pressureOptionsSchema.safeParse({ ...options, config_block: { reason: block.reason } })
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
