import { describe, expect, it } from "vitest";

import tokensSheet from "@/aurora-ui/shared/styles/tokens.css?raw";
import indexSheet from "@/index.css?raw";
import themeSource from "./theme.ts?raw";
import { themeSpec } from "./theme";

describe("editor theme", () => {
  it("uses theme variables for every colour and the monospace font", () => {
    const styles = Object.values(themeSpec);
    const colours = styles.flatMap((style) =>
      Object.entries(style)
        .filter(([property]) => /color$/i.test(property))
        .map(([, value]) => value),
    );
    expect(colours.length).toBeGreaterThan(0);
    for (const colour of colours) expect(colour).toMatch(/^var\(--[\w-]+\)$/);
    expect(themeSpec["&"].fontFamily).toBe("var(--font-mono)");
    expect(themeSpec["&"].backgroundColor).toBe("var(--background)");
    expect(themeSpec["&"].color).toBe("var(--foreground)");
  });

  // Token definitions are cross-checked in the next describe; this one checks the source
  // alone: no CodeMirror import and no colour literal.
  it("keeps the theme independent of CodeMirror and free of colour literals", () => {
    expect(themeSource).not.toContain("@codemirror");
    expect(themeSource).not.toMatch(/#[0-9a-f]{3,8}\b|\brgba?\(|\bhsla?\(/i);
  });
});

describe("editor theme tokens", () => {
  it("uses only variables that the dashboard's token sheets define", () => {
    const sheets = [indexSheet, tokensSheet].join("\n");
    expect(sheets).toContain("--foreground:");
    const used = [...themeSource.matchAll(/var\((--[\w-]+)\)/g)].map((match) => match[1]!);
    expect(used.length).toBeGreaterThan(10);
    const undefinedTokens = [...new Set(used)].filter(
      (name) => !new RegExp(`(?<![\\w-])${name}\\s*:`).test(sheets),
    );
    expect(undefinedTokens).toEqual([]);
  });

  it("overrides what CodeMirror's light base rules colour", () => {
    expect(themeSpec[".cm-content"].caretColor).toBe("var(--foreground)");
    expect(themeSpec["&.cm-focused"].outline).toContain("var(--ring)");
    expect(themeSpec[".cm-textfield"].backgroundColor).toBe("var(--background)");
    expect(themeSpec[".cm-textfield"].color).toBe("var(--foreground)");
    expect(themeSpec[".cm-button, .cm-button:active"].backgroundImage).toBe("none");
    expect(themeSpec[".cm-panels"].backgroundColor).toBe("var(--muted)");
  });

  it("selects text with a token that shows against the page", () => {
    const selection = themeSpec[".cm-content ::selection"].backgroundColor;
    expect(selection).toBe("var(--highlight)");
    expect(selection).not.toBe("var(--accent)");
  });

  it("marks validation lines with the destructive token", () => {
    expect(themeSpec[".cm-line-mark"].color).toBe("var(--destructive)");
    expect(themeSpec[".cm-marked-line"].background).toContain("var(--destructive)");
  });
});

describe("editor theme readability", () => {
  it("pairs the selection background with its foreground token", () => {
    const selection = themeSpec[".cm-content ::selection"];
    expect(selection.backgroundColor).toBe("var(--highlight)");
    expect(selection.color).toBe("var(--highlight-foreground)");
  });

  it("sets the monospace face on the scroller, where CodeMirror's base theme sets it", () => {
    expect(themeSpec[".cm-scroller"].fontFamily).toBe("var(--font-mono)");
    expect(themeSpec["&"].fontFamily).toBe("var(--font-mono)");
  });

  it("sets the search panel's controls in the body face at 12px", () => {
    for (const rule of [themeSpec[".cm-button, .cm-button:active"], themeSpec[".cm-textfield"]]) {
      expect(rule.fontSize).toBe("12px");
      expect(rule.fontFamily).toBe("var(--font-sans)");
    }
  });
});

describe("editor theme live preview", () => {
  it("sets prose in the body face only in live preview, keeping the scroller monospace", () => {
    expect(themeSpec["&.cm-live-preview .cm-content"].fontFamily).toBe("var(--font-sans)");
    expect(themeSpec[".cm-scroller"].fontFamily).toBe("var(--font-mono)");
  });

  it("sizes headings like the reading view", () => {
    expect(themeSpec[".cm-lp-h1"]).toMatchObject({ fontSize: "1.5em", fontWeight: "650" });
    expect(themeSpec[".cm-lp-h2"]).toMatchObject({ fontSize: "1.25em", fontWeight: "650" });
    expect(themeSpec[".cm-lp-h3"].fontSize).toBe("1.08em");
    expect(themeSpec[".cm-lp-h4"].fontSize).toBe("1em");
    expect(themeSpec[".cm-lp-h5, .cm-lp-h6"]).toMatchObject({
      fontSize: "0.9em",
      color: "var(--muted-foreground)",
    });
  });

  it("puts code on the muted ground and dims table pipes", () => {
    expect(themeSpec[".cm-lp-code"].backgroundColor).toBe("var(--muted)");
    expect(themeSpec[".cm-lp-code"].fontFamily).toBe("var(--font-mono)");
    expect(themeSpec[".cm-lp-dim"].color).toBe("var(--muted-foreground)");
  });
});
