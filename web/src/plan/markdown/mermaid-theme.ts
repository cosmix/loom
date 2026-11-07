import { useSyncExternalStore } from "react";

/**
 * Mermaid's `base` theme, fed from the planner's live CSS tokens so a diagram carries the active
 * theme's colours. Mermaid parses colours with khroma, which does not read `oklch()`, so each
 * token is resolved to a hex string first.
 */

const FALLBACK_INK = "#1f2328";
const FALLBACK_GROUND = "#f4f4f0";

/** Resolves any CSS colour to `#rrggbb` by painting one pixel; the raw value if canvas is absent. */
function toHex(css: string): string {
  try {
    const ctx = document.createElement("canvas").getContext("2d", { willReadFrequently: true });
    if (!ctx) return css;
    ctx.clearRect(0, 0, 1, 1);
    ctx.fillStyle = "#000";
    ctx.fillStyle = css;
    ctx.fillRect(0, 0, 1, 1);
    const [r = 0, g = 0, b = 0] = ctx.getImageData(0, 0, 1, 1).data;
    return `#${[r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("")}`;
  } catch {
    return css;
  }
}

/** Resolves a token to the colour it computes to on the document root. */
function token(name: string, fallback: string): string {
  const probe = document.createElement("span");
  probe.style.color = `var(${name}, ${fallback})`;
  probe.style.display = "none";
  document.documentElement.appendChild(probe);
  const resolved = getComputedStyle(probe).color;
  probe.remove();
  return toHex(resolved || fallback);
}

export function isDarkDocument(): boolean {
  return document.documentElement.classList.contains("dark");
}

export function mermaidThemeVariables(): Record<string, string | boolean> {
  const background = token("--background", FALLBACK_GROUND);
  const foreground = token("--foreground", FALLBACK_INK);
  const muted = token("--muted", background);
  const mutedForeground = token("--muted-foreground", foreground);
  const border = token("--border", mutedForeground);
  const card = token("--card", background);
  const accent = token("--primary", mutedForeground);
  const accentInk = token("--primary-foreground", foreground);
  const font = getComputedStyle(document.documentElement).getPropertyValue("--font-sans").trim();
  return {
    darkMode: isDarkDocument(),
    fontFamily: font || "sans-serif",
    background,
    mainBkg: muted,
    primaryColor: muted,
    primaryTextColor: foreground,
    primaryBorderColor: mutedForeground,
    secondaryColor: accent,
    secondaryTextColor: accentInk,
    secondaryBorderColor: accent,
    tertiaryColor: card,
    tertiaryTextColor: foreground,
    tertiaryBorderColor: border,
    nodeBorder: mutedForeground,
    clusterBkg: card,
    clusterBorder: border,
    lineColor: mutedForeground,
    textColor: foreground,
    titleColor: foreground,
    edgeLabelBackground: background,
    noteBkgColor: card,
    noteTextColor: foreground,
    noteBorderColor: border,
  };
}

/**
 * Changes whenever the document's theme does. The theme provider applies the scheme and variants
 * as a class and data attributes on the root, so watching those sees every atom change after the
 * tokens have switched.
 */
function subscribe(onChange: () => void): () => void {
  const observer = new MutationObserver(onChange);
  observer.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["class", "data-light-theme", "data-dark-theme"],
  });
  return () => observer.disconnect();
}

function themeSignature(): string {
  const root = document.documentElement;
  return `${isDarkDocument() ? "dark" : "light"}:${root.dataset.lightTheme ?? ""}:${root.dataset.darkTheme ?? ""}`;
}

export function useThemeSignature(): string {
  return useSyncExternalStore(subscribe, themeSignature, () => "light::");
}
