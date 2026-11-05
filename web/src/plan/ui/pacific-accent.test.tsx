import { describe, expect, it } from "vitest";

import indexCss from "@/index.css?raw";
import tokensCss from "@/aurora-ui/shared/styles/tokens.css?raw";

type Oklch = { lightness: number; chroma: number; hue: number };

/** The named token of every innermost rule block whose selector names the Pacific theme. */
function pacificValues(name: string): string[] {
  const withoutComments = tokensCss.replace(/\/\*[\s\S]*?\*\//g, "");
  const blocks = [...withoutComments.matchAll(/([^{}]*)\{([^{}]*)\}/g)];
  return blocks
    .filter(([, selector]) => selector!.includes('[data-dark-theme="blue"]'))
    .flatMap(([, , body]) => {
      const match = new RegExp(`(?:^|;)\\s*${name}\\s*:\\s*([^;]+)`).exec(body!);
      return match ? [match[1]!.trim()] : [];
    });
}

function parseOklch(value: string): Oklch {
  const match = /^oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)\s*\)$/.exec(value);
  if (!match) throw new Error(`${value} is not oklch(L C H)`);
  return { lightness: Number(match[1]), chroma: Number(match[2]), hue: Number(match[3]) };
}

function linearSrgb({ lightness, chroma, hue }: Oklch): [number, number, number] {
  const radians = (hue * Math.PI) / 180;
  const a = chroma * Math.cos(radians);
  const b = chroma * Math.sin(radians);
  const l = (lightness + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (lightness - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (lightness - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}

function luminance(colour: Oklch): number {
  const [red, green, blue] = linearSrgb(colour).map((channel) => Math.max(0, Math.min(1, channel)));
  return 0.2126 * red! + 0.7152 * green! + 0.0722 * blue!;
}

function contrast(a: Oklch, b: Oklch): number {
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (light! + 0.05) / (dark! + 0.05);
}

function only(name: string): Oklch {
  const values = pacificValues(name);
  expect(values, name).toHaveLength(1);
  return parseOklch(values[0]!);
}

describe("the Pacific accent", () => {
  it("is a bright colour inside the sRGB gamut, off the navy's own hue", () => {
    const primary = only("--primary");
    expect(primary.chroma).toBeGreaterThanOrEqual(0.12);
    for (const channel of linearSrgb(primary)) {
      expect(channel).toBeGreaterThanOrEqual(0);
      expect(channel).toBeLessThanOrEqual(1);
    }
    const background = only("--background");
    const apart = Math.abs(((primary.hue - background.hue + 540) % 360) - 180);
    expect(apart, "degrees between the accent and the navy").toBeGreaterThanOrEqual(30);
    expect(primary.hue).toBeGreaterThanOrEqual(298);
    expect(primary.hue).toBeLessThanOrEqual(312);
  });

  it("sits at least 13 degrees from every dark document hue", () => {
    const primary = only("--primary");
    const block = /\.dark\s*\{([^{}]*--doc-1:[^{}]*)\}/.exec(
      indexCss.replace(/\/\*[\s\S]*?\*\//g, ""),
    );
    const hues = [...(block?.[1] ?? "").matchAll(/--doc-\d+:\s*(oklch\([^)]*\))/g)].map(
      ([, value]) => parseOklch(value!).hue,
    );
    expect(hues).toHaveLength(8);
    for (const hue of hues) {
      const apart = Math.abs(((primary.hue - hue + 540) % 360) - 180);
      expect(apart, `degrees between the accent and hue ${hue}`).toBeGreaterThanOrEqual(13);
    }
  });

  it("carries its foreground at 4.5:1 and stands on the navy at 3:1", () => {
    const primary = only("--primary");
    expect(contrast(only("--primary-foreground"), primary)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(primary, only("--background"))).toBeGreaterThanOrEqual(3);
    expect(contrast(primary, only("--card"))).toBeGreaterThanOrEqual(3);
    expect(contrast(primary, only("--muted"))).toBeGreaterThanOrEqual(3);
  });

  it("is the focus ring", () => {
    expect(only("--ring")).toEqual(only("--primary"));
  });
});
