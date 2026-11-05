// Shared helpers for web/scripts/plan-journeys.mjs: results, page watching, waiting, the spine.

import { execFileSync } from "node:child_process";
import { mkdirSync, readdirSync } from "node:fs";
import { join } from "node:path";

export const FEATURE = "feat-planner";
export const ROOT = `/f/${FEATURE}`;
export const SPEC_DIR = `doc/product/${FEATURE}`;
export const SECTION_5 = `${SPEC_DIR}/section_05.md`;
export const NO_TMUX = "tmux cannot create a socket here";

export const STEP_TIMEOUT_MS = 10_000;
export const SPINE_SELECTOR = 'nav[aria-label="Feature documents"]';
const POLL_MS = 200;

/** Thrown by a journey that cannot run yet: a route or element a later stage owns does not exist. */
export class Pending extends Error {}

/** The one line a journey prints: any FAIL wins, then any PENDING, else PASS. */
export class Results {
  parts = new Map();

  add(id, status, note) {
    if (!this.parts.has(id)) this.parts.set(id, []);
    this.parts.get(id).push({ status, note });
  }

  /** Run one journey (or one viewport of it); a throw becomes FAIL or PENDING, never an abort. */
  async run(id, fn) {
    try {
      this.add(id, "PASS", (await fn()) ?? "");
    } catch (error) {
      if (error instanceof Pending) return this.add(id, "PENDING", error.message);
      const text = error instanceof Error ? error.message : String(error);
      const lines = text.split("\n");
      // Playwright's first line names the action; its call log names the locator that never came.
      const waiting = lines.find((line) => /waiting for|intercepts pointer/.test(line));
      this.add(id, "FAIL", [lines[0], waiting?.trim()].filter(Boolean).join(" "));
    }
  }

  lines() {
    const ids = [...this.parts.keys()].sort();
    return ids.map((id) => {
      const parts = this.parts.get(id);
      const status = ["FAIL", "PENDING"].find((s) => parts.some((p) => p.status === s)) ?? "PASS";
      const shown = parts.filter((p) => p.status === status || status === "PASS");
      const text = [...new Set(shown.map((p) => p.note).filter(Boolean))].join("; ");
      return { id, status, text: `${status} ${id} ${text}`.trimEnd() };
    });
  }
}

/** A journey's screenshots: `<out-dir>/<id>-<nn>-<name>.png`, one per step. */
export function shooter(outDir, id) {
  mkdirSync(outDir, { recursive: true });
  let n = 0;
  return async (page, name) => {
    n += 1;
    const file = join(outDir, `${id}-${String(n).padStart(2, "0")}-${name}.png`);
    await page.screenshot({ path: file });
  };
}

let lastSession = null;

/** The server reported a chat session and that session is over (`isActiveSession` in session-home.ts). */
export const chatEnded = () => ["ended", "failed"].includes(lastSession?.state);

function noteSession(payload) {
  try {
    const frame = JSON.parse(payload);
    if (frame?.type === "chat" && frame.session) lastSession = frame.session;
  } catch {
    // A frame that is not JSON carries no session.
  }
}

/**
 * Collect console and page errors, every frame the page sends over a websocket, and the chat
 * session the server last reported. Nothing is filtered: an error that fires on a clean page is a
 * finding to report.
 */
export function watchPage(page, errors, frames) {
  page.on("console", (msg) => {
    if (msg.type() === "error") errors.push(`console: ${msg.text()} (${msg.location().url})`);
  });
  page.on("pageerror", (err) => errors.push(`pageerror: ${err.message}`));
  page.on("websocket", (ws) => {
    ws.on("framesent", (frame) => {
      if (typeof frame.payload === "string") frames.push(frame.payload);
    });
    ws.on("framereceived", (frame) => {
      if (typeof frame.payload === "string") noteSession(frame.payload);
    });
  });
}

/** Sent chat commands (`start`, `send`, ...), parsed. */
export function chatFrames(frames) {
  const parsed = [];
  for (const raw of frames) {
    try {
      const frame = JSON.parse(raw);
      if (frame?.type === "chat") parsed.push(frame);
    } catch {
      // A frame that is not JSON is not a chat command.
    }
  }
  return parsed;
}

/** Poll a condition outside the page (files, sockets, processes) until it holds or time runs out. */
export async function until(label, check, timeoutMs = STEP_TIMEOUT_MS) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const value = await check();
    if (value) return value;
    if (Date.now() > deadline) throw new Error(`timed out after ${timeoutMs} ms: ${label}`);
    await new Promise((resolve) => setTimeout(resolve, POLL_MS));
  }
}

/** The app root rendered and no loading marker left (plan/ui/load-state.tsx). */
export async function settle(page) {
  await page.waitForSelector("#root > *");
  await page.waitForFunction(
    () => !document.querySelector('[role="status"][aria-label^="Loading"]'),
  );
}

/** Things the run saw that are not failures but the operator should read; printed after the lines. */
export const notes = new Set();

const POPOVER = '[data-slot="popover-content"]';

/**
 * A popover that opens by itself (the activity pill's rebuild prompt does when summaries are out of
 * date) covers the page and swallows clicks. Dismiss it as a user would, and say it happened.
 */
async function dismissPopovers(page, path) {
  if ((await page.locator(POPOVER).count()) === 0) return;
  const name = await page.locator(POPOVER).first().getAttribute("aria-label");
  notes.add(`the "${name}" popover was open on arrival at ${path}; dismissed with Escape`);
  await page.keyboard.press("Escape");
  await page.locator(POPOVER).first().waitFor({ state: "detached" });
}

export async function visit(page, origin, path) {
  const response = await page.goto(origin + path, { waitUntil: "load" });
  if (response && response.status() >= 400) {
    throw new Error(`${path} answered ${response.status()}`);
  }
  await settle(page);
  await dismissPopovers(page, path);
}

/** The spine, opened behind its "Documents" button: narrow, the nav shows with its body closed. */
export async function openSpine(page) {
  const nav = page.locator(SPINE_SELECTOR);
  if ((await nav.count()) === 0) {
    throw new Error(`no ${SPINE_SELECTOR} on ${new URL(page.url()).pathname}`);
  }
  const closed = nav.locator('.plan-spine__menu[aria-expanded="false"]:visible');
  if (await closed.count()) await closed.first().click();
  return nav.first();
}

/**
 * Expand every collapsed group so each document row is a visible link. Only the groups' own
 * disclosure buttons count: the narrow "Documents" control also carries aria-expanded, is first in
 * the nav, and is display: none at desktop widths, so clicking it there times out.
 */
export async function expandGroups(nav) {
  for (let i = 0; i < 20; i += 1) {
    const closed = nav.locator('.plan-spine__disclose[aria-expanded="false"]').first();
    if ((await closed.count()) === 0) return;
    await closed.click();
  }
}

/** The hrefs of the spine's rows; the next-step block's own links are not rows. */
export async function spineHrefs(nav) {
  const hrefs = await nav
    .locator("a[href]:not(.plan-spine-next a)")
    .evaluateAll((links) => links.map((a) => a.getAttribute("href")));
  return [...new Set(hrefs.map((h) => h.split("#")[0]))];
}

/** Click the spine row whose href satisfies `match`; returns false when the spine has none. */
export async function clickSpineRow(page, match) {
  const nav = await openSpine(page);
  await expandGroups(nav);
  const hrefs = await spineHrefs(nav);
  const href = hrefs.find(match);
  if (href === undefined) return false;
  await nav
    .locator(`a[href=${JSON.stringify(href)}]`)
    .first()
    .click();
  return href;
}

export function pathOf(url) {
  return decodeURIComponent(new URL(url, "http://x").pathname);
}

// tmux and the fake claude --------------------------------------------------------------------

/**
 * Where tmux keeps its sockets; `loom plan` names its own `loom-plan-<id>`. scripts/plan-journeys.sh
 * points TMUX_TMPDIR at a directory of this run's own, so every socket here is one it started; run
 * without it, the directory is the operator's and `killSockets` could end a chat that is theirs.
 */
export function planSockets() {
  const dir = join(process.env.TMUX_TMPDIR ?? "/tmp", `tmux-${process.getuid()}`);
  try {
    return readdirSync(dir).filter((name) => name.startsWith("loom-plan-"));
  } catch {
    return [];
  }
}

export function newSockets(before) {
  return planSockets().filter((name) => !before.includes(name));
}

export function killSockets(names) {
  for (const name of names) {
    try {
      execFileSync("tmux", ["-L", name, "kill-server"], { stdio: "ignore" });
    } catch {
      // Already gone.
    }
  }
}

/** Processes of the fake claude (scripts/plan-journeys.sh): `exec sleep 98765`. */
export function fakeClaudePids() {
  try {
    return execFileSync("pgrep", ["-f", "^sleep 98765$"], { encoding: "utf8" })
      .split("\n")
      .filter(Boolean);
  } catch {
    return [];
  }
}

// What the routed page shows ---------------------------------------------------------------------

// The routed page is `.plan-layout__page`; `main` also holds the spine, whose own text and
// open/closed menu would count as content and make a stale page look changed. A route outside a
// feature has no layout, so it falls back to `main`. The lookup is repeated inside each browser
// function below because Playwright serialises them and a module constant would not travel.

/** The state of the routed page: waiting (null), real content, or the catch-all. */
const PAGE_STATE = ({ prev }) => {
  if (document.querySelector('[role="status"][aria-label^="Loading"]')) return null;
  const root = document.querySelector(".plan-layout__page") ?? document.querySelector("main");
  if (!root) return null;
  const text = root.innerText.trim();
  const heading = root.querySelector('h1,h2,h3,h4,[role="heading"]');
  if (heading && text.length >= 200 && text !== prev) return "content";
  const why = [...root.querySelectorAll('.plan-empty,[data-empty],[role="status"]')].find(
    (e) => e.textContent.trim().length > 10 && !e.textContent.includes("No such page"),
  );
  return why ? "empty-state" : null;
};

/** Why a button is aria-disabled (DisabledReason draws the reason and describes the button by it). */
export async function blockedBy(page, button) {
  if ((await button.getAttribute("aria-disabled")) !== "true") return null;
  const id = await button.getAttribute("aria-describedby");
  return id ? await page.locator(`[id=${JSON.stringify(id)}]`).innerText() : "no reason shown";
}

export const mainText = (page) =>
  page.evaluate(
    () =>
      (document.querySelector(".plan-layout__page") ?? document.querySelector("main"))?.innerText ??
      "",
  );

export async function pageState(page, href, prev) {
  const arg = { prev };
  try {
    return await (await page.waitForFunction(PAGE_STATE, arg)).jsonValue();
  } catch {
    const seen = await page.evaluate(() => {
      const root = document.querySelector(".plan-layout__page") ?? document.querySelector("main");
      return {
        main: root !== null,
        headings: root ? root.querySelectorAll("h1,h2,h3,h4").length : 0,
        chars: root ? root.innerText.trim().length : 0,
        nope: (root?.innerText ?? "").includes("No such page"),
      };
    });
    const why = seen.main
      ? `the page holds ${seen.headings} headings and ${seen.chars} characters${seen.nope ? ' and says "No such page"' : ""}`
      : "no main element or routed page";
    throw new Error(`${href}: ${why}`);
  }
}

// One offer drawn twice ---------------------------------------------------------------------------

export const CHAT_ASIDE = 'aside[aria-label="Chat"]';

/** Every visible button's label with how many copies sit outside and inside the chat aside. */
export const BUTTON_LABELS = (chatSelector) => {
  const shown = (el) =>
    el.checkVisibility({ checkVisibilityCSS: true }) && el.getClientRects().length > 0;
  const labels = {};
  for (const b of [...document.querySelectorAll("button")].filter(shown)) {
    const text = b.innerText.replace(/\s+/g, " ").trim();
    if (!text) continue;
    const seen = (labels[text] ??= { outside: 0, chat: 0 });
    if (b.closest(chatSelector)) seen.chat += 1;
    else seen.outside += 1;
  }
  return labels;
};

/**
 * Repeated labels on one route. A label with one copy outside the chat aside and one inside is the
 * chat panel still drawing the offer (chat-redesign removes it): pending. Any other repeat is a fail.
 */
export function repeatedLabels(href, labels) {
  const fails = [];
  const pending = [];
  for (const [label, { outside, chat }] of Object.entries(labels)) {
    if (outside > 1 || (outside === 0 && chat > 1)) {
      fails.push(`${href}: "${label}" appears ${outside + chat} times`);
    } else if (outside === 1 && chat >= 1) {
      pending.push(`chat panel still draws ${label} (chat-redesign)`);
    }
  }
  return { fails, pending };
}
