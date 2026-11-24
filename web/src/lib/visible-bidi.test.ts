import { describe, expect, it } from "vitest";

import { codePointLabel, visibleBidi } from "@/lib/visible-bidi";

const BIDI = [
  0x061c,
  0x200e,
  0x200f,
  ...Array.from({ length: 5 }, (_, i) => 0x202a + i),
  ...Array.from({ length: 4 }, (_, i) => 0x2066 + i),
];

describe("visibleBidi", () => {
  it("labels every bidirectional control", () => {
    for (const code of BIDI) {
      const hex = code.toString(16).toUpperCase().padStart(4, "0");
      expect(visibleBidi(`a${String.fromCodePoint(code)}b`)).toBe(`a⟨U+${hex}⟩b`);
    }
  });

  it("leaves other format characters untouched", () => {
    for (const code of [0x200b, 0x200c, 0x200d, 0x00ad, 0x2060, 0xfeff]) {
      const text = `a${String.fromCodePoint(code)}b`;
      expect(visibleBidi(text)).toBe(text);
    }
  });

  it("formats a code point label with four upper-case hex digits", () => {
    expect(codePointLabel("­")).toBe("⟨U+00AD⟩");
    expect(codePointLabel("\u{1F600}")).toBe("⟨U+1F600⟩");
  });
});
