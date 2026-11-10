import { cleanup, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it } from "vitest";

import tokensDark from "@/aurora-ui/shared/styles/tokens-dark.css?raw";
import tokensLight from "@/aurora-ui/shared/styles/tokens-light.css?raw";
import { THEME_OPTIONS, ThemePicker, type ThemeOption } from "@/components/theme-picker";

// The palettes, light themes first as the stylesheet imports them.
const tokensCss = `${tokensLight}\n${tokensDark}`;

const TOKENS = {
  background: "--background",
  card: "--card",
  foreground: "--foreground",
  mutedForeground: "--muted-foreground",
  border: "--border",
  primary: "--primary",
  primaryForeground: "--primary-foreground",
} as const;
const TOKEN_ENTRIES = Object.entries(TOKENS) as [keyof typeof TOKENS, string][];

/// The tokens-light.css / tokens-dark.css rule that owns each offered theme's palette.
const OWNING_SELECTOR: Record<ThemeOption["id"], string> = {
  ledger: ":root",
  harbor: ':root[data-light-theme="harbor"]:not(.dark)',
  aubergine: '.dark[data-dark-theme="aubergine"]',
  graphite: '.dark[data-dark-theme="gray"]',
  midnight: '.dark[data-dark-theme="midnight"]',
  obsidian: '.dark[data-dark-theme="obsidian"]',
  indigo: '.dark[data-dark-theme="indigo"]',
};

interface Rule {
  selectors: string[];
  declarations: Map<string, string>;
}

function parseRules(css: string): Rule[] {
  const source = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const rules: Rule[] = [];
  for (const [, prelude, body] of source.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const selector = prelude!.trim();
    if (selector.startsWith("@")) continue;
    const declarations = new Map<string, string>();
    for (const declaration of body!.split(";")) {
      const colon = declaration.indexOf(":");
      if (colon === -1) continue;
      declarations.set(
        declaration.slice(0, colon).trim(),
        declaration
          .slice(colon + 1)
          .trim()
          .replace(/\s+/g, " "),
      );
    }
    rules.push({ selectors: selector.split(",").map((part) => part.trim()), declarations });
  }
  return rules;
}

/// The custom properties `element` itself receives. Later rules win: the theme
/// rules in the token files run from least to most specific, so source order agrees
/// with the cascade here. `web/scripts/theme-preview-test.mjs` checks the real
/// cascade in Chrome.
function resolveTokens(element: Element, rules: Rule[]): Map<string, string | undefined> {
  const resolved = new Map<string, string | undefined>();
  for (const rule of rules) {
    if (!rule.selectors.some((selector) => element.matches(selector))) continue;
    for (const [, token] of TOKEN_ENTRIES) {
      if (rule.declarations.has(token)) resolved.set(token, rule.declarations.get(token));
    }
  }
  return resolved;
}

function owningRule(rules: Rule[], selector: string): Rule {
  const rule = rules.find((candidate) => candidate.selectors.includes(selector));
  if (rule === undefined) throw new Error(`the token files have no ${selector} rule`);
  return rule;
}

function palette(rule: Rule): Map<string, string | undefined> {
  return new Map(TOKEN_ENTRIES.map(([, token]) => [token, rule.declarations.get(token)]));
}

function previewOf(option: ThemeOption): HTMLElement {
  const button = screen.getByRole("button", { name: `${option.label} theme` });
  return button.querySelector<HTMLElement>(".theme-preview")!;
}

/// The attributes ThemeProvider writes to <html> for an option.
function applyToRoot(option: ThemeOption): HTMLElement {
  const root = document.documentElement;
  root.classList.toggle("dark", option.scheme === "dark");
  root.dataset.lightTheme = option.lightVariant ?? "ledger";
  root.dataset.darkTheme = option.darkVariant ?? "aubergine";
  return root;
}

function renderPicker() {
  render(
    <Provider store={createStore()}>
      <ThemePicker />
    </Provider>,
  );
}

afterEach(() => {
  cleanup();
  document.documentElement.className = "";
  document.documentElement.removeAttribute("data-light-theme");
  document.documentElement.removeAttribute("data-dark-theme");
});

describe("theme picker palettes", () => {
  const rules = parseRules(tokensCss);

  it.each(THEME_OPTIONS)("$label previews and applies its token-file palette", (option) => {
    renderPicker();
    const expected = palette(owningRule(rules, OWNING_SELECTOR[option.id]));

    for (const value of expected.values()) expect(value).toBeTruthy();
    expect(resolveTokens(previewOf(option), rules)).toEqual(expected);
    expect(resolveTokens(applyToRoot(option), rules)).toEqual(expected);
  });

  it("gives every option a distinct palette", () => {
    const palettes = THEME_OPTIONS.map((option) =>
      JSON.stringify([...palette(owningRule(rules, OWNING_SELECTOR[option.id]))]),
    );

    expect(new Set(palettes).size).toBe(THEME_OPTIONS.length);
  });

  it("paints every swatch role from the matching CSS variable", () => {
    for (const option of THEME_OPTIONS) {
      for (const [role, token] of TOKEN_ENTRIES) {
        expect(option.palette[role]).toBe(`var(${token})`);
      }
    }
  });

  it("moves the preview and the applied theme together when the source changes", () => {
    renderPicker();
    const graphite = THEME_OPTIONS.find((option) => option.id === "graphite")!;
    const edited = parseRules(tokensCss);
    owningRule(edited, OWNING_SELECTOR.graphite).declarations.set("--primary", "oklch(0 0 0)");

    expect(resolveTokens(previewOf(graphite), edited).get("--primary")).toBe("oklch(0 0 0)");
    expect(resolveTokens(applyToRoot(graphite), edited).get("--primary")).toBe("oklch(0 0 0)");
  });

  it("detects a preview scope dropped from the light palette selector", () => {
    renderPicker();
    const ledger = THEME_OPTIONS.find((option) => option.id === "ledger")!;
    const edited = parseRules(tokensCss);
    const rule = owningRule(edited, OWNING_SELECTOR.ledger);
    rule.selectors = rule.selectors.filter((selector) => selector !== ".theme-preview");

    expect(resolveTokens(previewOf(ledger), edited)).not.toEqual(palette(rule));
  });
});
