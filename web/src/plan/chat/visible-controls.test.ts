import { cleanup, render } from "@testing-library/react";
import { createElement } from "react";
import { afterEach, describe, expect, it } from "vitest";

import { Untrusted, visibleControls } from "./visible-controls";

afterEach(cleanup);

describe("visibleControls", () => {
  it("escapes every listed format control and other BMP and supplementary format characters", () => {
    const codePoints = [
      0x202a, 0x202b, 0x202c, 0x202d, 0x202e, 0x2066, 0x2067, 0x2068, 0x2069, 0x200b, 0x200c,
      0x200d, 0x200e, 0x200f, 0x061c, 0xfeff, 0x00ad, 0x2060, 0x110bd, 0xe0001,
    ];
    const expected = [
      "⟨U+202A⟩",
      "⟨U+202B⟩",
      "⟨U+202C⟩",
      "⟨U+202D⟩",
      "⟨U+202E⟩",
      "⟨U+2066⟩",
      "⟨U+2067⟩",
      "⟨U+2068⟩",
      "⟨U+2069⟩",
      "⟨U+200B⟩",
      "⟨U+200C⟩",
      "⟨U+200D⟩",
      "⟨U+200E⟩",
      "⟨U+200F⟩",
      "⟨U+061C⟩",
      "⟨U+FEFF⟩",
      "⟨U+00AD⟩",
      "⟨U+2060⟩",
      "⟨U+110BD⟩",
      "⟨U+E0001⟩",
    ].join("");

    expect(visibleControls(`before${String.fromCodePoint(...codePoints)}after`)).toBe(
      `before${expected}after`,
    );
  });

  it("leaves plain text unchanged", () => {
    const text = `Hello, κόσμε! שלום 😀 e${String.fromCodePoint(0x0301)}\n\t<path> & ⟨U+202E⟩`;
    expect(visibleControls(text)).toBe(text);
    expect(visibleControls("")).toBe("");
  });
});

describe("Untrusted", () => {
  it("renders untrusted text in an isolated span without a raw right-to-left override", () => {
    const rlo = String.fromCodePoint(0x202e);
    const { container } = render(createElement(Untrusted, { text: `<b>${rlo}path</b>` }));
    const span = container.querySelector("span")!;

    expect(span.textContent).toBe("<b>⟨U+202E⟩path</b>");
    expect(span.textContent).not.toContain(rlo);
    expect(span.childElementCount).toBe(0);
    expect(span.style.unicodeBidi).toBe("isolate");
    expect(span.style.direction).toBe("ltr");
  });

  it("keeps a command that starts with an RTL letter in its executed order", () => {
    // `plaintext` ignores `direction` and lays such a paragraph out right-to-left, so the
    // displayed segment order would differ from the order the shell runs.
    const { container } = render(
      createElement(Untrusted, { text: "'\u05e9'; rm -rf doc/product; '\u05e9'; git status" }),
    );
    const span = container.querySelector("span")!;

    expect(span.style.unicodeBidi).toBe("isolate");
    expect(span.style.unicodeBidi).not.toBe("plaintext");
    expect(span.style.direction).toBe("ltr");
  });
});
