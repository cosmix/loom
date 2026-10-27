import { describe, expect, it } from "vitest";

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

  // Vitest serves CSS `?raw` imports as "" without `css` processing, so token
  // definitions are not cross-checked here; the source must hold no colour literal.
  it("keeps the theme independent of CodeMirror and free of colour literals", () => {
    expect(themeSource).not.toContain("@codemirror");
    expect(themeSource).not.toMatch(/#[0-9a-f]{3,8}\b|\brgba?\(|\bhsla?\(/i);
  });
});
