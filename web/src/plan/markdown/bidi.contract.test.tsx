import { cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { renderMarkdown } from "@/plan/markdown/render-test-utils";

// Control characters are built at run time so the file itself holds none of them.
const RLO = String.fromCodePoint(0x202e);
const LRI = String.fromCodePoint(0x2066);
const ZWJ = String.fromCodePoint(0x200d);
const FAMILY = [0x1f468, 0x1f469, 0x1f467].map((point) => String.fromCodePoint(point)).join(ZWJ);
const visible = (hex: string) => `⟨U+${hex}⟩`;

afterEach(() => {
  cleanup();
});

describe("markdown bidi contract", () => {
  it("a right-to-left override in body text is shown as a visible control", () => {
    const source = [
      `a${RLO}b`,
      "",
      `Inline \`x${LRI}y\` code.`,
      "",
      "```",
      `fenced ${RLO} body`,
      "```",
      "",
      `Family ${FAMILY} here.`,
      "",
    ].join("\n");

    const text = renderMarkdown(source).container.textContent ?? "";

    expect(text).toContain(visible("202E"));
    expect(text).toContain(visible("2066"));
    expect(text).not.toContain(RLO);
    expect(text).not.toContain(LRI);
    expect(text).toContain(FAMILY);
    expect(text).not.toContain(visible("200D"));
  });
});
