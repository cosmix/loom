import { describe, expect, it } from "vitest";

import { stageStatusSchema, type StageStatus } from "@/api/schema";
import { LEGEND, STAGE_STATES } from "@/lib/states";

describe("shared stage state table", () => {
  it("contains all thirteen legend rows", () => {
    expect(LEGEND).toHaveLength(13);
  });

  it("has exactly the schema's statuses in both directions", () => {
    const stateKeys = Object.keys(STAGE_STATES) as StageStatus[];

    for (const status of stateKeys) {
      expect(stageStatusSchema.options).toContain(status);
    }
    for (const status of stageStatusSchema.options) {
      expect(stateKeys).toContain(status);
    }
  });

  it("does not contain empty labels or legend text", () => {
    for (const state of LEGEND) {
      expect(state.label.trim()).not.toBe("");
      expect(state.legend.trim()).not.toBe("");
    }
  });
});
