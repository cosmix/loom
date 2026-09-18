import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it } from "vitest";

import { darkVariantAtom, lightVariantAtom, themeAtom } from "@/aurora-ui/shared/atoms/theme";
import { ThemeProvider } from "@/aurora-ui/theme/ThemeProvider";
import { THEME_OPTIONS, ThemePicker } from "@/components/theme-picker";

const THEME_LABELS = ["Ledger", "Aubergine", "Pacific", "Graphite"] as const;

const APPLIED_THEMES = [
  {
    label: "Ledger",
    scheme: "light",
    dark: false,
    datasetKey: "lightTheme",
    variant: "ledger",
    storageKey: "loom:light-variant",
  },
  {
    label: "Aubergine",
    scheme: "dark",
    dark: true,
    datasetKey: "darkTheme",
    variant: "aubergine",
    storageKey: "loom:dark-variant",
  },
  {
    label: "Pacific",
    scheme: "dark",
    dark: true,
    datasetKey: "darkTheme",
    variant: "blue",
    storageKey: "loom:dark-variant",
  },
  {
    label: "Graphite",
    scheme: "dark",
    dark: true,
    datasetKey: "darkTheme",
    variant: "gray",
    storageKey: "loom:dark-variant",
  },
] as const;

const NAMED_PALETTES = [
  {
    label: "Ledger",
    background: "oklch(0.925 0.009 102)",
    primary: "oklch(0.84 0.155 80)",
  },
  {
    label: "Aubergine",
    background: "oklch(0.16 0.035 325)",
    primary: "oklch(0.83 0.165 82)",
  },
  {
    label: "Pacific",
    background: "oklch(0.18 0.045 258)",
    primary: "oklch(0.78 0.135 258)",
  },
  {
    label: "Graphite",
    background: "oklch(0.18 0.005 270)",
    primary: "oklch(0.78 0.015 270)",
  },
] as const;

function renderThemePicker(store = createStore(), applyTheme = false) {
  const picker = <ThemePicker />;
  const content = applyTheme ? <ThemeProvider>{picker}</ThemeProvider> : picker;

  return {
    store,
    ...render(<Provider store={store}>{content}</Provider>),
  };
}

afterEach(() => {
  cleanup();
  localStorage.clear();
  document.documentElement.className = "";
  document.documentElement.removeAttribute("data-light-theme");
  document.documentElement.removeAttribute("data-dark-theme");
});

describe("ThemePicker", () => {
  it("renders four buttons with the option names", () => {
    renderThemePicker();

    expect(screen.getAllByRole("button")).toHaveLength(4);
    for (const label of THEME_LABELS) {
      expect(screen.getByRole("button", { name: `${label} theme` })).toBeTruthy();
    }
  });

  it("selects the Pacific dark variant", () => {
    const { store } = renderThemePicker();

    fireEvent.click(screen.getByRole("button", { name: "Pacific theme" }));

    expect(store.get(themeAtom)).toBe("dark");
    expect(store.get(darkVariantAtom)).toBe("blue");
  });

  it("selects the Ledger light variant", () => {
    const store = createStore();
    store.set(themeAtom, "dark");
    store.set(lightVariantAtom, "gray");
    renderThemePicker(store);

    fireEvent.click(screen.getByRole("button", { name: "Ledger theme" }));

    expect(store.get(themeAtom)).toBe("light");
    expect(store.get(lightVariantAtom)).toBe("ledger");
  });

  it("presses only the clicked theme", () => {
    renderThemePicker();
    const graphite = screen.getByRole("button", { name: "Graphite theme" });

    fireEvent.click(graphite);

    const pressed = screen
      .getAllByRole("button")
      .filter((button) => button.getAttribute("aria-pressed") === "true");
    expect(pressed).toEqual([graphite]);
  });

  it("does not expose radio controls", () => {
    renderThemePicker();

    expect(screen.queryAllByRole("radio")).toHaveLength(0);
  });

  it("renders the approved swatch viewport for every theme", () => {
    renderThemePicker();

    for (const label of THEME_LABELS) {
      const button = screen.getByRole("button", { name: `${label} theme` });
      const swatch = button.querySelector("svg");
      expect(swatch).not.toBeNull();
      expect(swatch?.getAttribute("viewBox")).toBe("0 0 120 76");
    }
  });

  it("exports complete and distinct oklch palettes", () => {
    const paletteValues = THEME_OPTIONS.flatMap((option) => Object.values(option.palette));
    const backgrounds = THEME_OPTIONS.map((option) => option.palette.background);

    expect(paletteValues).toHaveLength(28);
    expect(paletteValues.every((value) => value.startsWith("oklch("))).toBe(true);
    expect(new Set(backgrounds).size).toBe(4);
  });

  it("does not mark an unoffered dark variant as active", () => {
    const store = createStore();
    store.set(themeAtom, "dark");
    store.set(darkVariantAtom, "green");
    renderThemePicker(store);

    const pressed = screen
      .getAllByRole("button")
      .filter((button) => button.getAttribute("aria-pressed") === "true");
    expect(pressed).toHaveLength(0);
  });

  it.each(APPLIED_THEMES)(
    "every theme applies $label and restores from storage",
    async ({ label, scheme, dark, datasetKey, variant, storageKey }) => {
      const store = createStore();
      store.set(themeAtom, scheme === "light" ? "dark" : "light");
      const { unmount } = renderThemePicker(store, true);

      fireEvent.click(screen.getByRole("button", { name: `${label} theme` }));

      await waitFor(() => {
        expect(document.documentElement.classList.contains("dark")).toBe(dark);
        expect(document.documentElement.dataset[datasetKey]).toBe(variant);
        expect(localStorage.getItem("loom:theme")).toBe(JSON.stringify(scheme));
        expect(localStorage.getItem(storageKey)).toBe(JSON.stringify(variant));
      });

      unmount();
      renderThemePicker(createStore(), true);

      await waitFor(() => {
        const restored = screen.getByRole("button", { name: `${label} theme` });
        expect(restored.getAttribute("aria-pressed")).toBe("true");
      });
    },
  );

  it.each(NAMED_PALETTES)(
    "uses the named palette literals for $label",
    ({ label, background, primary }) => {
      const option = THEME_OPTIONS.find((candidate) => candidate.label === label);

      expect(option?.palette.background).toBe(background);
      expect(option?.palette.primary).toBe(primary);
    },
  );

  it("draws the complete stage graph in every swatch", () => {
    renderThemePicker();

    for (const label of THEME_LABELS) {
      const button = screen.getByRole("button", { name: `${label} theme` });
      const swatch = button.querySelector("svg");
      expect(swatch?.querySelectorAll("rect")).toHaveLength(15);
      expect(swatch?.querySelectorAll("path")).toHaveLength(7);
    }
  });
});
