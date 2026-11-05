// Run with: bun scripts/theme-preview-test.mjs
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { chromium } from "playwright-core";

const css = await readFile(
  new URL("../src/aurora-ui/shared/styles/tokens.css", import.meta.url),
  "utf8",
);
const browser = await chromium.launch({
  executablePath: process.env.CHROME_PATH ?? "/usr/bin/google-chrome",
  args: ["--no-sandbox"],
});
try {
  const page = await browser.newPage();
  await page.setContent(`<style>${css}</style><span class="theme-preview"></span>`);
  for (const [scheme, variant] of [
    ["light", "ledger"],
    ["dark", "walnut"],
    ["dark", "gray"],
  ]) {
    const colors = await page.evaluate(
      ([scheme, variant]) => {
        const root = document.documentElement;
        const preview = document.querySelector(".theme-preview");
        root.classList.toggle("dark", scheme === "dark");
        preview.classList.toggle("dark", scheme === "dark");
        root.dataset.lightTheme = preview.dataset.lightTheme =
          scheme === "light" ? variant : "ledger";
        root.dataset.darkTheme = preview.dataset.darkTheme = scheme === "dark" ? variant : "walnut";
        const names = [
          "background",
          "card",
          "foreground",
          "muted-foreground",
          "border",
          "primary",
          "primary-foreground",
        ];
        const applied = names.map((name) =>
          getComputedStyle(root).getPropertyValue(`--${name}`).trim(),
        );
        // All options remain visible even when another theme is active.
        root.classList.toggle("dark", scheme !== "dark");
        return names.map((name, index) => [
          applied[index],
          getComputedStyle(preview).getPropertyValue(`--${name}`).trim(),
        ]);
      },
      [scheme, variant],
    );
    for (const [applied, preview] of colors) {
      assert.ok(applied);
      assert.equal(preview, applied, `${scheme}/${variant}`);
    }
  }
  console.log("theme previews match applied CSS palettes");
} finally {
  await browser.close();
}
