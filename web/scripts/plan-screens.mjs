// Screenshot planner routes in a real browser.
//
// Usage: node web/scripts/plan-screens.mjs [--themes=<id>,<id>] <token-url> <out-dir> <route>...
//
// Each route is shot at 1440x900 and 390x844, in Ledger (light) and Walnut (dark) unless
// --themes names others from THEMES, full page, to <out-dir>/<route-slug>-<width>-<theme>.png.
// Exit 1 on any console error, page error, HTTP status >= 400 on a route, or a navigation that
// never renders #root.
// Driven by scripts/plan-screens.sh, which starts the server (loopback is per Bash call).

import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { chromium } from "playwright-core";

const VIEWPORTS = [
  { width: 1440, height: 900 },
  { width: 390, height: 844 },
];

// Same ids and variants as the options in web/src/components/theme-picker.tsx; the storage keys in
// themeInitScript match web/src/aurora-ui/shared/atoms/theme.ts (jotai atomWithStorage stores JSON).
const THEMES = {
  ledger: { name: "ledger", scheme: "light", dark: "walnut" },
  walnut: { name: "walnut", scheme: "dark", dark: "walnut" },
  graphite: { name: "graphite", scheme: "dark", dark: "gray" },
};
const DEFAULT_THEMES = ["ledger", "walnut"];

const ROOT_TIMEOUT_MS = 15_000;
const SETTLE_MS = 400;
const MAX_GROW_PX = 6000;

/** `/` becomes `home`, `/f/feat-planner` becomes `f-feat-planner`. */
function routeSlug(route) {
  const slug = route
    .replace(/^\/+|\/+$/g, "")
    .replace(/[^A-Za-z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug === "" ? "home" : slug;
}

function themeInitScript(theme) {
  return `
    localStorage.setItem("loom:theme", ${JSON.stringify(JSON.stringify(theme.scheme))});
    localStorage.setItem("loom:light-variant", ${JSON.stringify(JSON.stringify("ledger"))});
    localStorage.setItem("loom:dark-variant", ${JSON.stringify(JSON.stringify(theme.dark))});
  `;
}

function watchErrors(page, errors) {
  page.on("console", (msg) => {
    if (msg.type() === "error") errors.push(`console: ${msg.text()}`);
  });
  page.on("pageerror", (err) => errors.push(`pageerror: ${err.message}`));
}

/** Open the token URL so the server sets its cookies, then return the app origin. */
async function bootstrap(page, tokenUrl) {
  await page.goto(tokenUrl, { waitUntil: "load" });
  await page.waitForSelector("#root > *", { timeout: ROOT_TIMEOUT_MS });
  return new URL(page.url()).origin;
}

async function shoot(page, url, file, errors) {
  const response = await page.goto(url, { waitUntil: "load" });
  if (response && response.status() >= 400) {
    errors.push(`navigation: ${url} answered ${response.status()}`);
  }
  try {
    await page.waitForSelector("#root > *", { timeout: ROOT_TIMEOUT_MS });
  } catch {
    errors.push(`navigation: ${url} never rendered the app root`);
  }
  // Content arrives over the websocket after the shell renders; LoadState (plan/ui/load-state.tsx)
  // is the loading marker. A shot taken before it clears shows only skeletons.
  try {
    await page.waitForFunction(
      () => !document.querySelector('[role="status"][aria-label^="Loading"]'),
      undefined,
      { timeout: ROOT_TIMEOUT_MS },
    );
  } catch {
    errors.push(`navigation: ${url} still loading after ${ROOT_TIMEOUT_MS} ms`);
  }
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(SETTLE_MS);
  await growToContainers(page);
  await page.screenshot({ path: file, fullPage: true });
}

/**
 * The shell fills the viewport and scrolls inside panes, so `fullPage` alone clips at the
 * viewport height. Grow the viewport by the tallest pane overflow (capped) to show the content.
 */
async function growToContainers(page) {
  const extra = await page.evaluate(() => {
    let max = 0;
    for (const el of document.querySelectorAll("#root *")) {
      const overflowY = getComputedStyle(el).overflowY;
      if (overflowY !== "auto" && overflowY !== "scroll") continue;
      max = Math.max(max, el.scrollHeight - el.clientHeight);
    }
    return max;
  });
  if (extra <= 0) return;
  const { width, height } = page.viewportSize();
  await page.setViewportSize({ width, height: height + Math.min(extra, MAX_GROW_PX) });
  await page.waitForTimeout(SETTLE_MS);
}

async function shootTheme(browser, theme, viewport, ctx) {
  const context = await browser.newContext({ viewport });
  await context.addInitScript(themeInitScript(theme));
  const page = await context.newPage();
  const errors = [];
  watchErrors(page, errors);
  const origin = await bootstrap(page, ctx.tokenUrl);
  for (const route of ctx.routes) {
    const file = join(ctx.outDir, `${routeSlug(route)}-${viewport.width}-${theme.name}.png`);
    const before = errors.length;
    await page.setViewportSize(viewport);
    await shoot(page, origin + route, file, errors);
    console.log(`${errors.length === before ? "ok  " : "FAIL"} ${file}`);
  }
  await context.close();
  return errors;
}

async function main() {
  const args = process.argv.slice(2);
  const themeIds = args[0]?.startsWith("--themes=")
    ? args.shift().slice("--themes=".length).split(",")
    : DEFAULT_THEMES;
  const themes = themeIds.map((id) => THEMES[id]);
  const [tokenUrl, outDir, ...routes] = args;
  if (themes.includes(undefined) || !tokenUrl || !outDir || routes.length === 0) {
    console.error(
      `usage: node web/scripts/plan-screens.mjs [--themes=<id>,<id>] <token-url> <out-dir> <route>...; themes: ${Object.keys(THEMES).join(", ")}`,
    );
    process.exit(2);
  }
  mkdirSync(outDir, { recursive: true });
  const browser = await chromium.launch({ args: ["--no-sandbox"] });
  const failures = [];
  try {
    for (const theme of themes) {
      for (const viewport of VIEWPORTS) {
        const errors = await shootTheme(browser, theme, viewport, {
          tokenUrl,
          outDir,
          routes,
        });
        for (const e of errors) {
          failures.push(`[${theme.name} ${viewport.width}] ${e}`);
        }
      }
    }
  } finally {
    await browser.close();
  }
  for (const f of failures) console.error(f);
  process.exit(failures.length === 0 ? 0 : 1);
}

await main();
