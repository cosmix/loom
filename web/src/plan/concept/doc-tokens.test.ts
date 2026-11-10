import { describe, expect, it } from "vitest";
import tokensDark from "@/aurora-ui/shared/styles/tokens-dark.css?raw";
import tokensLight from "@/aurora-ui/shared/styles/tokens-light.css?raw";
import indexCss from "@/index.css?raw";

// The palettes, light themes first as the stylesheet imports them.
const tokensCss = `${tokensLight}\n${tokensDark}`;

const themes = [":root", ".dark"] as const;
// The `--background` each doc token set sits on: `.dark` has no background of its own, each dark
// variant sets one.
const backgroundSelectors: Record<(typeof themes)[number], string[]> = {
  ":root": [":root", ':root[data-light-theme="harbor"]:not(.dark)'],
  ".dark": ["aubergine", "gray", "midnight", "obsidian", "indigo"].map(
    (id) => `.dark[data-dark-theme="${id}"]`,
  ),
};
const docTokens = Array.from({ length: 8 }, (_, index) => `--doc-${index + 1}`);

type Oklch = { lightness: number; chroma: number; hue: number };

function declaration(css: string, selector: string, name: string): string | undefined {
  const withoutComments = css.replace(/\/\*[\s\S]*?\*\//g, "");
  for (const block of withoutComments.matchAll(/([^{}]+)\{([^}]*)\}/g)) {
    if (!block[1].split(",").some((entry) => entry.trim() === selector)) continue;
    const value = block[2].match(new RegExp(`(?:^|;)\\s*${name}\\s*:\\s*([^;]+)`, "m"));
    if (value) return value[1].trim();
  }
  return undefined;
}

function parseOklch(value: string | undefined, label: string): Oklch {
  const number = "[-+]?(?:\\d*\\.)?\\d+";
  const match = value?.match(
    new RegExp(`^oklch\\(\\s*(${number})\\s+(${number})\\s+(${number})(?:deg)?\\s*\\)$`, "i"),
  );
  if (!match) throw new Error(`${label} must be an OKLCH value; got ${value ?? "missing"}`);
  return { lightness: Number(match[1]), chroma: Number(match[2]), hue: Number(match[3]) };
}

function luminance({ lightness, chroma, hue }: Oklch): number {
  const radians = (hue * Math.PI) / 180;
  const a = chroma * Math.cos(radians);
  const b = chroma * Math.sin(radians);
  const l = (lightness + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (lightness - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (lightness - 0.0894841775 * a - 1.291485548 * b) ** 3;
  // Clip the converted linear sRGB channels to the display gamut before measuring them.
  const clamp = (channel: number) => Math.max(0, Math.min(1, channel));
  const red = clamp(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s);
  const green = clamp(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s);
  const blue = clamp(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s);
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}

describe("document colour tokens", () => {
  it("each token defined in :root and .dark", () => {
    for (const theme of themes) {
      for (const token of docTokens) {
        const value = declaration(indexCss, theme, token);
        expect(value, `${theme} ${token}`).toBeDefined();
        parseOklch(value, `${theme} ${token}`);
      }
    }
  });

  it("no hue in 50-90", () => {
    for (const theme of themes) {
      for (const token of docTokens) {
        const value = declaration(indexCss, theme, token);
        const { hue } = parseOklch(value, `${theme} ${token}`);
        const normalizedHue = ((hue % 360) + 360) % 360;
        expect(normalizedHue < 50 || normalizedHue > 90, `${theme} ${token}: ${value}`).toBe(true);
      }
    }
  });

  it("contrast at least 3:1 against every theme's --background", () => {
    for (const theme of themes) {
      for (const selector of backgroundSelectors[theme]) {
        const background = luminance(
          parseOklch(declaration(tokensCss, selector, "--background"), `${selector} --background`),
        );
        for (const token of docTokens) {
          const value = declaration(indexCss, theme, token);
          const foreground = luminance(parseOklch(value, `${theme} ${token}`));
          const contrast =
            (Math.max(foreground, background) + 0.05) / (Math.min(foreground, background) + 0.05);
          expect(
            contrast,
            `${selector} ${token}: ${value}, contrast ${contrast.toFixed(2)}:1`,
          ).toBeGreaterThanOrEqual(3);
        }
      }
    }
  });
});
