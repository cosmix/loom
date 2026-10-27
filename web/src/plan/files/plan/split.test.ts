import { describe, expect, it } from "vitest";

import { splitPlan } from "./split";

describe("splitPlan", () => {
  it("preserves the pressure record and its original heading line", () => {
    const text =
      "# Plan\n\n<!-- loom METADATA -->\n```yaml\nstages: []\n```\n<!-- END loom METADATA -->\n\n## Pressure tests\nPassed.\n";
    const result = splitPlan(text, 3);
    expect(result).toEqual({
      prose: "# Plan\n\n",
      prose_offset: 0,
      record: "\n## Pressure tests\nPassed.\n",
      record_offset: 7,
    });
    expect(
      result.record_offset +
        result.record.split("\n").findIndex((line) => line === "## Pressure tests") +
        1,
    ).toBe(9);
  });

  it("handles a block without a record and preserves CRLF", () => {
    const text =
      "# Plan\r\n<!-- loom METADATA -->\r\n```yaml\r\nstages: []\r\n```\r\n<!-- END loom METADATA -->\r\n";
    expect(splitPlan(text, 2)).toEqual({
      prose: "# Plan\r\n",
      prose_offset: 0,
      record: "",
      record_offset: 6,
    });
  });

  it("keeps the entire document when no usable block is identified", () => {
    const text = "# Plan\nBody\n";
    const whole = { prose: text, prose_offset: 0, record: "", record_offset: 0 };
    expect(splitPlan(text, null)).toEqual(whole);
    expect(splitPlan(text, 0)).toEqual(whole);
    expect(splitPlan(text, 10)).toEqual(whole);
    expect(splitPlan(text, 2)).toEqual(whole);
  });

  it("ignores an end marker before the metadata line", () => {
    const text =
      "<!-- END loom METADATA -->\n<!-- loom METADATA -->\n<!-- END loom METADATA -->\n## Pressure tests";
    expect(splitPlan(text, 2)).toEqual({
      prose: "<!-- END loom METADATA -->\n",
      prose_offset: 0,
      record: "## Pressure tests",
      record_offset: 3,
    });
  });
});
