import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, beforeEach, expect, it } from "vitest";

import { darkVariantAtom, themeAtom } from "@/aurora-ui/shared/atoms/theme";
import tokensCss from "@/aurora-ui/shared/styles/tokens.css?raw";
import { ThemeProvider } from "@/aurora-ui/theme/ThemeProvider";
import { ThemePicker } from "@/components/theme-picker";
import indexCss from "@/index.css?raw";

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

const GRAY = '.dark[data-dark-theme="gray"]';
const CHAINS = {
  Ledger: [":root"],
  Walnut: [".dark", ":root"],
  Graphite: [GRAY, ".dark", ":root"],
} as const;
type Theme = keyof typeof CHAINS;
const THEMES = Object.keys(CHAINS) as Theme[];
const DARK_THEMES: Theme[] = ["Walnut", "Graphite"];

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
  Walnut: {
    "--background": [0.19, 0.014, 55],
    "--card": [0.22, 0.015, 55],
    "--border": [0.31, 0.014, 55],
    "--foreground": [0.92, 0.012, 80],
    "--muted-foreground": [0.71, 0.014, 60],
  },
  Graphite: {
    "--background": [0.2, 0.003, 270],
    "--card": [0.23, 0.003, 270],
    "--border": [0.32, 0.004, 270],
    "--foreground": [0.93, 0.004, 80],
    "--muted-foreground": [0.71, 0.005, 270],
  },
};

const triple = ({ lightness, chroma, hue }: Oklch) => [lightness, chroma, hue];

it("the theme picker offers Ledger Walnut and Graphite (the dashboard default dark is Aubergine)", async () => {
  const store = createStore();
  expect(store.get(darkVariantAtom)).toBe("aubergine");
  store.set(themeAtom, "dark");
  store.set(darkVariantAtom, "walnut");
  render(
    <Provider store={store}>
      <ThemeProvider>
        <ThemePicker themes={["ledger", "walnut", "graphite"]} />
      </ThemeProvider>
    </Provider>,
  );
  const group = screen.getByRole("group", { name: "theme" });
  const labels = within(group)
    .getAllByRole("button")
    .map((button) => button.getAttribute("aria-label"));
  expect(labels).toEqual(["Ledger theme", "Walnut theme", "Graphite theme"]);
  const walnut = within(group).getByRole("button", { name: "Walnut theme" });
  const graphite = within(group).getByRole("button", { name: "Graphite theme" });
  expect(walnut.getAttribute("aria-pressed")).toBe("true");
  await waitFor(() => expect(document.documentElement.dataset.darkTheme).toBe("walnut"));

  fireEvent.click(graphite);
  await waitFor(() => {
    expect(document.documentElement.dataset.darkTheme).toBe("gray");
    expect(localStorage.getItem("loom:dark-variant")).toBe(JSON.stringify("gray"));
  });
  expect(graphite.getAttribute("aria-pressed")).toBe("true");
  expect(walnut.getAttribute("aria-pressed")).not.toBe("true");
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

  const retired = ["blue", "green", "slate", "sand"].map((id) => `data-dark-theme="${id}"`);
  for (const rule of [...rules(tokensCss), ...indexRules]) {
    const prelude = rule.selectors.join(",");
    for (const marker of retired) expect(prelude, prelude).not.toContain(marker);
  }
});
