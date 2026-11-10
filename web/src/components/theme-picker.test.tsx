import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it } from "vitest";

import { darkVariantAtom, lightVariantAtom, themeAtom } from "@/aurora-ui/shared/atoms/theme";
import { ThemeProvider } from "@/aurora-ui/theme/ThemeProvider";
import { THEME_OPTIONS, ThemePicker } from "@/components/theme-picker";

const THEME_LABELS = [
  "Ledger",
  "Harbor",
  "Aubergine",
  "Graphite",
  "Midnight",
  "Obsidian",
  "Indigo Night",
] as const;

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
    label: "Graphite",
    scheme: "dark",
    dark: true,
    datasetKey: "darkTheme",
    variant: "gray",
    storageKey: "loom:dark-variant",
  },
  {
    label: "Harbor",
    scheme: "light",
    dark: false,
    datasetKey: "lightTheme",
    variant: "harbor",
    storageKey: "loom:light-variant",
  },
  {
    label: "Midnight",
    scheme: "dark",
    dark: true,
    datasetKey: "darkTheme",
    variant: "midnight",
    storageKey: "loom:dark-variant",
  },
  {
    label: "Obsidian",
    scheme: "dark",
    dark: true,
    datasetKey: "darkTheme",
    variant: "obsidian",
    storageKey: "loom:dark-variant",
  },
  {
    label: "Indigo Night",
    scheme: "dark",
    dark: true,
    datasetKey: "darkTheme",
    variant: "indigo",
    storageKey: "loom:dark-variant",
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
  it("renders seven buttons with the option names", () => {
    renderThemePicker();

    expect(screen.getAllByRole("button")).toHaveLength(7);
    for (const label of THEME_LABELS) {
      expect(screen.getByRole("button", { name: `${label} theme` })).toBeTruthy();
    }
  });

  it("scopes each swatch to the same theme selector as the applied theme", () => {
    renderThemePicker();
    for (const option of THEME_OPTIONS) {
      const button = screen.getByRole("button", { name: `${option.label} theme` });
      const scope = button.querySelector(".theme-preview") as HTMLElement;
      expect(scope.classList.contains("dark")).toBe(option.scheme === "dark");
      expect(scope.dataset.lightTheme ?? null).toBe(option.lightVariant);
      expect(scope.dataset.darkTheme ?? null).toBe(option.darkVariant);
      expect(option.palette.background).toBe("var(--background)");
    }
  });

  it("renders the custom swatch with each option palette instead of the stage graph", () => {
    render(
      <Provider store={createStore()}>
        <ThemePicker
          swatch={(palette) => (
            <span data-testid="custom-swatch" data-background={palette.background}>
              Custom swatch
            </span>
          )}
        />
      </Provider>,
    );

    expect(screen.getAllByText("Custom swatch")).toHaveLength(7);
    for (const option of THEME_OPTIONS) {
      const button = screen.getByRole("button", { name: `${option.label} theme` });
      const swatch = button.querySelector("[data-testid='custom-swatch']");
      expect(swatch?.getAttribute("data-background")).toBe(option.palette.background);
      expect(button.querySelector("svg")).toBeNull();
    }
  });

  it("marks every option button with the caller's class and adds none by default", () => {
    const { unmount } = render(
      <Provider store={createStore()}>
        <ThemePicker optionClassName="caller-option" />
      </Provider>,
    );
    const marked = screen
      .getAllByRole("button")
      .filter((b) => b.classList.contains("caller-option"));
    expect(marked).toHaveLength(THEME_OPTIONS.length);
    unmount();

    render(
      <Provider store={createStore()}>
        <ThemePicker />
      </Provider>,
    );
    for (const button of screen.getAllByRole("button")) {
      expect(button.className).not.toContain("undefined");
      expect(button.className.endsWith(" ")).toBe(false);
    }
  });

  it("selects the theme when a card has a custom swatch", () => {
    const store = createStore();
    store.set(themeAtom, "light");
    render(
      <Provider store={store}>
        <ThemePicker swatch={() => <span>Custom swatch</span>} />
      </Provider>,
    );
    const graphite = screen.getByRole("button", { name: "Graphite theme" });

    fireEvent.click(graphite);

    expect(store.get(themeAtom)).toBe("dark");
    expect(store.get(darkVariantAtom)).toBe("gray");
    expect(graphite.getAttribute("aria-pressed")).toBe("true");
  });

  it("selects the Graphite dark variant from the light scheme", () => {
    const store = createStore();
    store.set(themeAtom, "light");
    renderThemePicker(store);

    fireEvent.click(screen.getByRole("button", { name: "Graphite theme" }));

    expect(store.get(themeAtom)).toBe("dark");
    expect(store.get(darkVariantAtom)).toBe("gray");
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

  it("exports complete palettes backed by scoped CSS variables", () => {
    const paletteValues = THEME_OPTIONS.flatMap((option) => Object.values(option.palette));

    expect(paletteValues).toHaveLength(49);
    expect(paletteValues.every((value) => value.startsWith("var(--"))).toBe(true);
  });

  it("reads a stored dark variant no card offers as the default, Aubergine", () => {
    localStorage.setItem("loom:dark-variant", JSON.stringify("retired-variant"));
    const store = createStore();
    store.set(themeAtom, "dark");
    renderThemePicker(store);

    const pressed = screen
      .getAllByRole("button")
      .filter((button) => button.getAttribute("aria-pressed") === "true");
    expect(pressed.map((button) => button.getAttribute("aria-label"))).toEqual(["Aubergine theme"]);
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
