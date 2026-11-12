// Drive what a user does in the planner, in a real browser, against a real `loom plan` server.
//
// Usage: node web/scripts/plan-journeys.mjs [--strict] [--no-tmux] <token-url> <out-dir> <clone-path>
//
// One line per journey on stdout: `PASS J1 ...`, `FAIL J1 <why>` or `PENDING J1 <what is missing>`
// (J8 belongs to scripts/plan-journeys.sh, which also starts the server). A screenshot per step
// lands in <out-dir>. Exit 1 on any FAIL, console error or page error; --strict makes PENDING fail
// too. --no-tmux says tmux cannot create a socket here, so a journey that needs a session stops
// at that step with PENDING. Without --no-tmux, TMUX_TMPDIR must name a directory of this run's
// own (the script sets it) or the run refuses to start. <clone-path> is the scratch clone the
// server serves; J3 edits it.
//
// J1 and J2 are in plan-journeys-reading.mjs. J4 and J5 are in plan-journeys-chat.mjs, which
// lists the selectors chat-redesign must honour.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { chromium } from "playwright-core";
import { j4, j5 } from "./plan-journeys-chat.mjs";
import { j1, j2 } from "./plan-journeys-reading.mjs";
import {
  BUTTON_LABELS,
  CHAT_ASIDE,
  Pending,
  ROOT,
  Results,
  FEATURE,
  STEP_TIMEOUT_MS,
  blockedBy,
  notes,
  pageState,
  pathOf,
  repeatedLabels,
  settle,
  shooter,
  until,
  visit,
  watchPage,
} from "./plan-journeys-kit.mjs";

const errors = [];
const SERVER_READY_MS = 60_000;

// J3 --------------------------------------------------------------------------------------------

const DASHED = ".concept-mark[data-dashed='true']";

async function j3(env) {
  const { page, origin, shot, clone } = env;
  const rel = "product/feat-planner/section_03.md";
  const file = join(clone, "doc", rel);
  const marker = `journeymark${Date.now()}`;
  await visit(page, origin, `/files/${rel}`);
  await page.getByRole("link", { name: "Edit", exact: true }).click();
  const editor = page.locator(".cm-content");
  await editor.click();
  await page.keyboard.press("Control+End");
  await page.keyboard.type(marker);
  await shot(page, "edited");
  const save = page.getByRole("button", { name: "Save", exact: true });
  const blocked = await blockedBy(page, save);
  if (blocked !== null) throw new Error(`Save is blocked: ${blocked}`);
  await save.click();
  await until("the file in the clone holds the new word", () =>
    readFileSync(file, "utf8").includes(marker),
  );
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await page.waitForFunction((text) => document.body.innerText.includes(text), marker);
  await shot(page, "saved");
  await page.getByRole("link", { name: "Summary", exact: true }).first().click();
  await settle(page);
  const stale = () =>
    page.evaluate(
      (sel) =>
        document.querySelector(sel) !== null ||
        /rebuild|out of date/i.test(document.querySelector("main")?.innerText ?? ""),
      DASHED,
    );
  const seen = await until("a dashed mark or rebuild notice", stale, 8_000).then(
    () => "an out-of-date mark followed",
    () => "no dashed mark or rebuild notice within 8 s (the fake claude answers no rebuild)",
  );
  await shot(page, "summary-after-save");
  return `saved a word to ${rel}; ${seen}`;
}

// J6, J7 ----------------------------------------------------------------------------------------

const PRIMARIES = () => {
  const shown = (el) =>
    el.checkVisibility({ checkVisibilityCSS: true }) && el.getClientRects().length > 0;
  const primaries = [...document.querySelectorAll('[data-look="primary"]')]
    .filter(shown)
    .map((b) => {
      let top = Infinity;
      let bottom = -Infinity;
      for (const walker = document.createTreeWalker(b, NodeFilter.SHOW_TEXT); walker.nextNode();) {
        const range = document.createRange();
        range.selectNodeContents(walker.currentNode);
        for (const r of range.getClientRects())
          [top, bottom] = [Math.min(top, r.top), Math.max(bottom, r.bottom)];
      }
      const lines = (bottom - top) / parseFloat(getComputedStyle(b).fontSize);
      return {
        text: b.innerText.trim(),
        height: b.getBoundingClientRect().height,
        wrapped: lines > 1.9,
      };
    });
  return primaries;
};

async function j6(env) {
  const { page, origin, shot } = env;
  const fails = [];
  const pending = new Set();
  for (const href of env.routes) {
    await visit(page, origin, href);
    await pageState(page, href, "");
    const labels = await page.evaluate(BUTTON_LABELS, CHAT_ASIDE);
    const repeats = repeatedLabels(href, labels);
    fails.push(...repeats.fails);
    for (const note of repeats.pending) pending.add(note);
    for (const p of await page.evaluate(PRIMARIES)) {
      if (p.height > 40 || p.wrapped)
        fails.push(
          `${href}: primary "${p.text}" is ${Math.round(p.height)}px${p.wrapped ? " and wraps" : ""}`,
        );
    }
    await shot(
      page,
      `buttons-${pathOf(href).replace(/\W+/g, "-").replace(/^-|-$/g, "") || "home"}`,
    );
  }
  if (fails.length) throw new Error(fails.join(" | "));
  if (pending.size) throw new Pending([...pending].join("; "));
  return `${env.routes.size} routes, no repeated button label, no tall or wrapped primary`;
}

const NAMES = ({ feature }) => {
  const text = (sel) => [...document.querySelectorAll(sel)].map((e) => e.innerText).join("\n");
  // The app bar only: a document's own <header> keeps the document's own heading (common.md).
  const where = feature
    ? [".plan-header", 'nav[aria-label="Feature documents"]']
    : [".plan-header", ".home h1, .home h2, .home h3"];
  return {
    title: document.title,
    switcher: document.querySelector('button[aria-label^="Switch feature"]')?.textContent ?? "",
    named: where.filter((sel) => text(sel).includes("Loom Plan")),
  };
};

async function j7Names(env) {
  const { page, origin, shot } = env;
  const fails = [];
  for (const href of ["/", ...env.routes]) {
    await visit(page, origin, href);
    const feature = href.startsWith(ROOT);
    if (feature) {
      await page.waitForFunction((f) => document.title.startsWith(f), FEATURE).catch(() => {});
    }
    const seen = await page.evaluate(NAMES, { feature });
    if (feature && !seen.title.startsWith(FEATURE)) fails.push(`${href}: title is "${seen.title}"`);
    if (feature && !seen.switcher.includes(FEATURE))
      fails.push(`${href}: switcher shows "${seen.switcher.trim()}"`);
    if (seen.named.length) fails.push(`${href}: "Loom Plan" in ${seen.named.join(", ")}`);
  }
  await shot(page, "names");
  if (fails.length) throw new Error(fails.join(" | "));
  return "slug in title and switcher, no Loom Plan in the app bar, spine or home headings";
}

async function j7Layout(env) {
  const { page, origin, shot } = env;
  const fails = [];
  for (const href of ["/", ...env.routes]) {
    await visit(page, origin, href);
    const over = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    if (over > 0) fails.push(`${href}: scrolls sideways by ${over}px at 390`);
    await shot(page, `layout-${pathOf(href).replace(/\W+/g, "-").replace(/^-|-$/g, "") || "home"}`);
  }
  if (fails.length) throw new Error(fails.join(" | "));
  return "no route scrolls sideways at 390px";
}

// main ------------------------------------------------------------------------------------------

async function withPage(browser, viewport, tokenUrl, body) {
  const context = await browser.newContext({ viewport });
  context.setDefaultTimeout(STEP_TIMEOUT_MS);
  const page = await context.newPage();
  const frames = [];
  watchPage(page, errors, frames);
  try {
    await page.goto(tokenUrl, { waitUntil: "load" });
    const origin = new URL(page.url()).origin;
    // `loom plan` holds every /api request until its project load is ready (about 10 s for a
    // debug build on this repo's doc tree, bounded by the server's 60 s LOAD_WAIT). Wait for it
    // once, so a journey's first step does not spend its whole timeout on the load.
    await page.request.get(`${origin}/api/features`, { timeout: SERVER_READY_MS });
    await page.waitForSelector("#root > *");
    await body({ page, frames, origin });
  } finally {
    await context.close();
  }
}

async function wide(env, results) {
  const step = (id, fn) => results.run(id, () => fn({ ...env, shot: shooter(env.outDir, id) }));
  await step("J1", (e) => j1(e, 1440));
  if (env.routes.size === 0) env.routes.add(`${ROOT}/product`);
  await step("J2", j2);
  await step("J3", j3);
  await step("J6", j6);
  await step("J7", j7Names);
  await step("J4", j4);
  await step("J5", j5);
}

async function main() {
  const args = process.argv.slice(2);
  const flags = new Set(args.filter((a) => a.startsWith("--")));
  const [tokenUrl, outDir, clone] = args.filter((a) => !a.startsWith("--"));
  if (
    !tokenUrl ||
    !outDir ||
    !clone ||
    [...flags].some((f) => !["--strict", "--no-tmux"].includes(f))
  ) {
    console.error(
      "usage: node web/scripts/plan-journeys.mjs [--strict] [--no-tmux] <token-url> <out-dir> <clone-path>",
    );
    process.exit(2);
  }
  // Without it the sockets directory is the operator's, and `killSockets` could end their chat.
  if (!flags.has("--no-tmux") && !process.env.TMUX_TMPDIR) {
    console.error(
      "TMUX_TMPDIR is unset: run scripts/plan-journeys.sh, set TMUX_TMPDIR to a directory of this run's own, or pass --no-tmux",
    );
    process.exit(2);
  }
  const results = new Results();
  const env = { outDir, clone, routes: new Set(), tmuxOk: !flags.has("--no-tmux") };
  const browser = await chromium.launch({ args: ["--no-sandbox"] });
  try {
    await withPage(browser, { width: 1440, height: 900 }, tokenUrl, async (page) => {
      await wide({ ...env, ...page }, results);
    });
    await withPage(browser, { width: 390, height: 844 }, tokenUrl, async (page) => {
      const narrow = { ...env, ...page };
      await results.run("J1", () => j1({ ...narrow, routes: new Set() }, 390));
      await results.run("J7", () => j7Layout({ ...narrow, shot: shooter(outDir, "J7-390") }));
    });
  } finally {
    await browser.close();
  }
  let failed = errors.length > 0;
  for (const { status, text } of results.lines()) {
    console.log(text);
    if (status === "FAIL" || (status === "PENDING" && flags.has("--strict"))) failed = true;
  }
  for (const e of errors) console.error(`ERROR ${e}`);
  for (const note of notes) console.error(`NOTE ${note}`);
  process.exit(failed ? 1 : 0);
}

await main();
