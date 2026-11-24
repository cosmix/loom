import { describe, expect, it } from "vitest";

import invalidFixture from "@/plan/api/fixtures/files-save-invalid.json";
import { saveResultSchema } from "@/plan/api/schema-files";
import { initialSaveFlow, saveFlow } from "@/plan/editor/save-flow";

const invalid = saveResultSchema.parse(invalidFixture);

describe("save flow contract", () => {
  it("an invalid verdict for older text does not offer a forced save of newer text", () => {
    let state = initialSaveFlow("original", "h1");
    state = saveFlow(state, { type: "change", text: "first" });
    state = saveFlow(state, { type: "save" });
    state = saveFlow(state, { type: "change", text: "second" });
    state = saveFlow(state, { type: "result", result: { ok: true, value: invalid } });

    expect(state.status).toBe("idle");
    expect(state.validation).toBeNull();
    expect(state.text).toBe("second");
  });
});
