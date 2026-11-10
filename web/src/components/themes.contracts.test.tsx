import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, beforeEach, expect, it } from "vitest";

import { darkVariantAtom, themeAtom } from "@/aurora-ui/shared/atoms/theme";
import tokensDark from "@/aurora-ui/shared/styles/tokens-dark.css?raw";
import tokensLight from "@/aurora-ui/shared/styles/tokens-light.css?raw";
import { ThemeProvider } from "@/aurora-ui/theme/ThemeProvider";
import { ThemePicker } from "@/components/theme-picker";
import indexCss from "@/index.css?raw";

// The palettes, light themes first as the stylesheet imports them.
const tokensCss = `${tokensLight}\n${tokensDark}`;

function resetDocument() {
  localStorage.clear();
  document.documentElement.className = "";
  document.documentElement.removeAttribute("data-light-theme");
  document.documentElement.removeAttribute("data-dark-theme");
}

beforeEach(resetDocument);
afterEach(() => {
  cleanup();
  resetDocument();
});

type Oklch = { lightness: number; chroma: number; hue: number };
type Rule = { selectors: string[]; body: string };

const darkVariant = (id: string) => `.dark[data-dark-theme="${id}"]`;
const CHAINS = {
  Ledger: [":root"],
  Harbor: [':root[data-light-theme="harbor"]:not(.dark)', ":root"],
  Graphite: [darkVariant("gray"), ".dark", ":root"],
  Midnight: [darkVariant("midnight"), ".dark", ":root"],
  Obsidian: [darkVariant("obsidian"), ".dark", ":root"],
  "Indigo Night": [darkVariant("indigo"), ".dark", ":root"],
} as const;
type Theme = keyof typeof CHAINS;
const THEMES = Object.keys(CHAINS) as Theme[];
const DARK_THEMES: Theme[] = ["Graphite"];
// The dark themes with the amber accent: their warning hazard must sit off it, as Aubergine's does.
const AMBER_DARK_THEMES: Theme[] = ["Midnight", "Obsidian", "Indigo Night"];

function rules(css: string): Rule[] {
  const bare = css.replace(/\/\*[\s\S]*?\*\//g, "");
  return Array.from(bare.matchAll(/([^{}]+)\{([^{}]*)\}/g), (block) => ({
    selectors: block[1].split(",").map((entry) => entry.trim()),
    body: block[2],
  }));
}

const ALL_RULES = rules(`${tokensCss}\n${indexCss}`);

/** The last declaration of `token` across every rule whose exact selector list holds `selector`. */
function declared(selector: string, token: string): string | undefined {
  let found: string | undefined;
  for (const rule of ALL_RULES) {
    if (!rule.selectors.includes(selector)) continue;
    for (const declaration of rule.body.split(";")) {
      const colon = declaration.indexOf(":");
      if (colon < 0 || declaration.slice(0, colon).trim() !== token) continue;
      found = declaration.slice(colon + 1).trim();
    }
  }
  return found;
}

function value(theme: Theme, token: string, depth = 0): string | undefined {
  for (const selector of CHAINS[theme]) {
    const found = declared(selector, token);
    if (found === undefined) continue;
    // A token that names another token resolves through the same theme chain, as the cascade does.
    const reference = found.match(/^var\(\s*(--[\w-]+)\s*\)$/);
    return reference && depth < 8 ? value(theme, reference[1], depth + 1) : found;
  }
  return undefined;
}

function oklch(theme: Theme, token: string): Oklch {
  const raw = value(theme, token);
  const number = "[-+]?(?:\\d*\\.)?\\d+";
  const match = raw?.match(
    new RegExp(
      `^oklch\\(\\s*(${number})\\s+(${number})\\s+(${number})(?:deg)?(?:\\s*/\\s*${number}%?)?\\s*\\)$`,
      "i",
    ),
  );
  if (!match) throw new Error(`${theme} ${token} must be an OKLCH value; got ${raw ?? "missing"}`);
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

function contrast(theme: Theme, foreground: string, background: string): number {
  const [one, two] = [luminance(oklch(theme, foreground)), luminance(oklch(theme, background))];
  return (Math.max(one, two) + 0.05) / (Math.min(one, two) + 0.05);
}

const TONES = [
  "executing",
  "completed",
  "blocked",
  "pending",
  "queued",
  "warning",
  "merged",
  "dimmed",
  "control",
  "contract",
].map((tone) => `--tone-${tone}`);

const QUIET_SURFACES = [
  "--background",
  "--card",
  "--popover",
  "--secondary",
  "--muted",
  "--accent",
  "--border",
  "--input",
  "--sidebar",
  "--sidebar-accent",
  "--sidebar-border",
];

const APPROVED: Record<string, Record<string, [number, number, number]>> = {
  Graphite: {
    "--background": [0.2, 0.003, 270],
    "--card": [0.23, 0.003, 270],
    "--border": [0.32, 0.004, 270],
    "--foreground": [0.93, 0.004, 80],
    "--muted-foreground": [0.71, 0.005, 270],
  },
  Midnight: {
    "--background": [0.17, 0.032, 255],
    "--card": [0.2, 0.034, 255],
    "--muted": [0.235, 0.035, 255],
    "--border": [0.305, 0.032, 255],
    "--foreground": [0.93, 0.012, 255],
    "--muted-foreground": [0.72, 0.016, 255],
    "--primary": [0.82, 0.15, 85],
    "--primary-foreground": [0.18, 0.032, 255],
  },
  Obsidian: {
    "--background": [0.13, 0.008, 260],
    "--card": [0.16, 0.008, 260],
    "--muted": [0.195, 0.009, 260],
    "--border": [0.265, 0.008, 260],
    "--foreground": [0.93, 0.006, 260],
    "--muted-foreground": [0.72, 0.012, 260],
    "--primary": [0.82, 0.15, 85],
    "--primary-foreground": [0.14, 0.008, 260],
  },
  "Indigo Night": {
    "--background": [0.17, 0.045, 280],
    "--card": [0.2, 0.047, 280],
    "--muted": [0.235, 0.049, 280],
    "--border": [0.305, 0.045, 280],
    "--foreground": [0.93, 0.012, 280],
    "--muted-foreground": [0.72, 0.023, 280],
    "--primary": [0.82, 0.15, 85],
    "--primary-foreground": [0.18, 0.045, 280],
  },
  Harbor: {
    "--background": [0.955, 0.014, 235],
    "--card": [0.985, 0.007, 235],
    "--muted": [0.925, 0.016, 235],
    "--border": [0.855, 0.015, 235],
    "--foreground": [0.22, 0.02, 235],
    "--muted-foreground": [0.46, 0.03, 235],
    "--primary": [0.46, 0.09, 205],
    "--primary-foreground": [0.985, 0.01, 235],
  },
};

const triple = ({ lightness, chroma, hue }: Oklch) => [lightness, chroma, hue];

it("the theme picker offers seven themes (the dashboard default dark is Aubergine)", async () => {
  const store = createStore();
  expect(store.get(darkVariantAtom)).toBe("aubergine");
  store.set(themeAtom, "dark");
  store.set(darkVariantAtom, "aubergine");
  render(
    <Provider store={store}>
      <ThemeProvider>
        <ThemePicker />
      </ThemeProvider>
    </Provider>,
  );
  const group = screen.getByRole("group", { name: "theme" });
  const labels = within(group)
    .getAllByRole("button")
    .map((button) => button.getAttribute("aria-label"));
  expect(labels).toEqual([
    "Ledger theme",
    "Harbor theme",
    "Aubergine theme",
    "Graphite theme",
    "Midnight theme",
    "Obsidian theme",
    "Indigo Night theme",
  ]);
  const aubergine = within(group).getByRole("button", { name: "Aubergine theme" });
  const graphite = within(group).getByRole("button", { name: "Graphite theme" });
  expect(aubergine.getAttribute("aria-pressed")).toBe("true");
  await waitFor(() => expect(document.documentElement.dataset.darkTheme).toBe("aubergine"));

  fireEvent.click(graphite);
  await waitFor(() => {
    expect(document.documentElement.dataset.darkTheme).toBe("gray");
    expect(localStorage.getItem("loom:dark-variant")).toBe(JSON.stringify("gray"));
  });
  expect(graphite.getAttribute("aria-pressed")).toBe("true");
  expect(aubergine.getAttribute("aria-pressed")).not.toBe("true");
});

it("the theme tokens give each theme readable text and the dark themes quiet surfaces and an orange accent", () => {
  for (const theme of THEMES) {
    expect(contrast(theme, "--foreground", "--background"), theme).toBeGreaterThanOrEqual(7);
    expect(contrast(theme, "--muted-foreground", "--card"), theme).toBeGreaterThanOrEqual(4.5);
    for (const tone of TONES) {
      expect(contrast(theme, tone, "--card"), `${theme} ${tone}`).toBeGreaterThanOrEqual(3);
    }
  }
  for (const theme of DARK_THEMES) {
    for (const token of ["--primary", "--ring"]) {
      expect(triple(oklch(theme, token)), `${theme} ${token}`).toEqual([0.74, 0.15, 55]);
    }
    expect(oklch(theme, "--primary-foreground"), theme).toEqual(oklch(theme, "--background"));
    for (const token of QUIET_SURFACES) {
      expect(oklch(theme, token).chroma, `${theme} ${token}`).toBeLessThanOrEqual(0.015);
    }
    for (const [token, expected] of Object.entries(APPROVED[theme]!)) {
      expect(triple(oklch(theme, token)), `${theme} ${token}`).toEqual(expected);
    }
    for (const token of ["--tone-warning", "--hazard-warning"]) {
      expect(oklch(theme, token).hue, `${theme} ${token}`).toBeGreaterThanOrEqual(70);
    }
  }
  for (const theme of AMBER_DARK_THEMES) {
    for (const token of ["--primary", "--ring"]) {
      expect(triple(oklch(theme, token)), `${theme} ${token}`).toEqual([0.82, 0.15, 85]);
    }
    const apart = Math.abs(oklch(theme, "--hazard-warning").hue - oklch(theme, "--primary").hue);
    expect(apart, `${theme} hazard-warning against the accent`).toBeGreaterThanOrEqual(25);
    expect(contrast(theme, "--primary-foreground", "--primary"), theme).toBeGreaterThanOrEqual(7);
    expect(contrast(theme, "--primary", "--background"), theme).toBeGreaterThanOrEqual(7);
  }
  for (const theme of ["Harbor", ...AMBER_DARK_THEMES] as Theme[]) {
    for (const [token, expected] of Object.entries(APPROVED[theme]!)) {
      expect(triple(oklch(theme, token)), `${theme} ${token}`).toEqual(expected);
    }
  }
  expect(contrast("Harbor", "--primary-foreground", "--primary")).toBeGreaterThanOrEqual(4.5);
  expect(contrast("Harbor", "--primary", "--background")).toBeGreaterThanOrEqual(4.5);
  expect(oklch("Harbor", "--ring")).toEqual(oklch("Harbor", "--primary"));
  expect(value("Harbor", "--accent-text")).toBe(value("Harbor", "--primary"));
  expect(value("Ledger", "--tone-warning")).toBe("oklch(0.55 0.15 50)");

  const indexRules = rules(indexCss);
  const accentInk = (selector: string) =>
    indexRules
      .filter((rule) => rule.selectors.includes(selector))
      .flatMap((rule) => rule.body.split(";"))
      .map((declaration) => declaration.split(/:(.*)/s))
      .filter(([name]) => name!.trim() === "--accent-ink")
      .map(([, text]) => text!.trim());
  expect(accentInk(":root")).toContain("var(--ring)");
  expect(accentInk(".dark")).toContain("var(--primary)");

  const retired = ["blue", "green", "slate", "sand", "walnut"].map(
    (id) => `data-dark-theme="${id}"`,
  );
  for (const rule of [...rules(tokensCss), ...indexRules]) {
    const prelude = rule.selectors.join(",");
    for (const marker of retired) expect(prelude, prelude).not.toContain(marker);
  }
});
