// The planner's chat in a real browser with no server behind it: Vite serves `web/src`, the REST
// routes answer from `src/plan/api/fixtures`, and the plan socket is played by this file. Used by
// chat-layout-test.mjs, which measures the panel at short window heights.

import { readFileSync } from "node:fs";
import { chromium } from "playwright-core";
import { createServer } from "vite";

const fixture = (name) =>
  JSON.parse(
    readFileSync(new URL(`../src/plan/api/fixtures/${name}.json`, import.meta.url), "utf8"),
  );

const PROJECT_KEY = "chat-layout";
const REST = {
  "/api/server": () => ({ ...fixture("server-info"), project_key: PROJECT_KEY, read_only: [] }),
  "/api/tree": () => fixture("files-tree"),
  "/api/pressure/options": () => fixture("pressure-options"),
  "/api/run/state": () => fixture("run-state"),
  "/api/summaries/panel": () => fixture("summary-panel"),
};

const PROSE = [
  "The bridge keeps one session per project and replays its transcript to every tab that joins.",
  "A tab that falls behind asks for a snapshot; the server answers with the turns it holds and the queue.",
  "Nothing is written to the plan until you settle the section, so a refused edit leaves the file as it was.",
].join(" ");

/** A multi-line draft with more attachment rows than the composer shows at once. */
export const DRAFT = {
  text: Array.from(
    { length: 9 },
    (_, i) => `Line ${i + 1} of a draft the user is still writing`,
  ).join("\n"),
  attachments: Array.from({ length: 6 }, (_, i) => ({
    kind: "file",
    id: `file-${i}`,
    name: `notes-${i}.md`,
    media_type: "text/markdown",
    bytes: 2048 * (i + 1),
  })),
};

function helloFrame() {
  const hello = fixture("hello");
  return {
    ...hello,
    server: { ...hello.server, project_key: PROJECT_KEY, read_only: [] },
    presence: { entries: [] },
    authoring: { holder: { ...hello.you, away_since: null, frees_at: null } },
  };
}

function turns(count) {
  const [base] = fixture("chat-snapshot").turns;
  return Array.from({ length: count }, (_, i) => ({
    ...base,
    id: `t${i}`,
    started_at: new Date(Date.UTC(2026, 9, 17, 8, i)).toISOString(),
    open: false,
    changed_files: [],
    summaries_rebuilding: [],
    items: [
      {
        type: "user",
        id: `u${i}`,
        text: `Question ${i + 1}: how does the bridge recover?`,
        attachments: [],
        delivery: "delivered",
        origin: null,
      },
      { type: "text", id: `a${i}`, markdown: `${PROSE}\n\n${PROSE}` },
    ],
  }));
}

function pendingPermission() {
  return {
    type: "permission",
    id: "permission-1",
    request_id: "request-1",
    tool: "Bash",
    input: JSON.stringify(
      { command: "cargo test --test plan_session_bridge_e2e", description: "Run the bridge tests" },
      null,
      2,
    ),
    status: "pending",
    deny_reason: null,
    note: null,
    expires_at: "2099-01-01T00:00:00Z",
    fetch: null,
  };
}

/**
 * The chat frames the layout is checked against: a long idle transcript, a session waiting on a
 * permission with a queue, and no session at all (the empty state).
 */
export const SCENARIOS = {
  long() {
    const base = fixture("chat-snapshot");
    return { session: { ...base.session, state: "idle", queued: 0 }, turns: turns(12), queue: [] };
  },
  decision() {
    const base = fixture("chat-snapshot");
    const all = turns(6);
    const last = all.at(-1);
    last.open = true;
    last.items.push(pendingPermission());
    const queue = Array.from({ length: 4 }, (_, i) => ({
      id: `queued-${i}`,
      first_line: `Queued message ${i + 1}`,
      text: `Queued message ${i + 1}\nwith a second line`,
    }));
    return {
      session: { ...base.session, state: "permission", queued: queue.length },
      turns: all,
      queue,
    };
  },
  empty() {
    return { session: null, turns: [], queue: [] };
  },
};

/** The chat column, wherever it is mounted: the panel (docked or in the sheet) or the kickoff. */
const COLUMN = ":is(.chat-panel, .kickoff-transcript)";

/** Vite on `web/src` in plan mode, on a free port. */
export async function startPlanner() {
  const server = await createServer({
    mode: "plan",
    logLevel: "error",
    server: { port: 0, open: false, proxy: {} },
  });
  await server.listen();
  return { origin: server.resolvedUrls.local[0].replace(/\/$/, ""), close: () => server.close() };
}

export function launchBrowser() {
  return chromium.launch({
    executablePath: process.env.CHROME_PATH ?? "/usr/bin/google-chrome",
    args: ["--no-sandbox"],
  });
}

async function mockServer(page, origin, scenario) {
  // Vite's fallback page is the dashboard; the planner server answers every route with plan.html.
  await page.route(
    (url) => !url.pathname.includes(".") && !url.pathname.startsWith("/api/"),
    (route) =>
      route.request().resourceType() === "document"
        ? route.continue({ url: `${origin}/plan.html` })
        : route.fallback(),
  );
  await page.route(
    (url) => url.pathname.startsWith("/api/"),
    (route) => {
      const reply = REST[new URL(route.request().url()).pathname];
      return route.fulfill({
        status: reply ? 200 : 404,
        contentType: "application/json",
        body: JSON.stringify(reply ? reply() : {}),
      });
    },
  );
  await page.routeWebSocket(
    (url) => url.pathname === "/ws",
    (socket) => {
      socket.send(JSON.stringify(helloFrame()));
      socket.send(
        JSON.stringify({
          type: "chat",
          frame: "snapshot",
          previous: null,
          ...SCENARIOS[scenario](),
        }),
      );
    },
  );
}

/** The kickoff conversation of the fixture's session: the chat column on the page, with no panel. */
export const KICKOFF = `/new?session=${fixture("chat-snapshot").session.id}`;

/**
 * Open `route` with the chat showing `scenario` and the saved draft in the composer. On a panel
 * route below 1280px the chat is a sheet, opened here from the header's Chat button.
 */
export async function openChat(
  browser,
  origin,
  { viewport, scenario, draft = DRAFT, route = "/files" },
) {
  const context = await browser.newContext({ viewport, reducedMotion: "reduce" });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await context.addInitScript(
    ([key, value]) => localStorage.setItem(key, value),
    [`loom-plan:${PROJECT_KEY}:draft`, JSON.stringify(draft)],
  );
  await mockServer(page, origin, scenario);
  await page.goto(origin + route, { waitUntil: "load" });
  if (route !== KICKOFF && viewport.width < 1280) await page.click("[data-chat-sheet-trigger]");
  await page.waitForSelector(`${COLUMN} .chat-composer-field`);
  await page.waitForSelector(scenario === "empty" ? ".chat-empty" : ".chat-transcript .chat-turn");
  await page.evaluate(() => document.fonts.ready);
  await settle(page);
  return { page, errors, close: () => context.close() };
}

/** Two frames: a resize observer answers a layout change in the frame after it. */
export function settle(page) {
  return page.evaluate(
    () => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))),
  );
}

/**
 * The boxes the layout rules are about, in viewport pixels, with what each one holds beyond its
 * own height. Runs in the page.
 */
export function measure() {
  const box = (node) => {
    if (!node) return null;
    const r = node.getBoundingClientRect();
    return {
      top: Math.round(r.top),
      bottom: Math.round(r.bottom),
      left: Math.round(r.left),
      right: Math.round(r.right),
      height: Math.round(r.height),
      scrollHeight: node.scrollHeight,
      clientHeight: node.clientHeight,
      scrollTop: Math.round(node.scrollTop),
    };
  };
  const column = document.querySelector(".chat-panel, .kickoff-transcript");
  const one = (selector) => box(column.querySelector(selector));
  return {
    viewport: { width: innerWidth, height: innerHeight },
    page: { scrollHeight: document.documentElement.scrollHeight },
    column: box(column),
    head: one(".chat-panel-top, .kickoff-head"),
    body: one(".chat-transcript-frame, .chat-empty"),
    log: one(".chat-transcript"),
    composer: one(".chat-composer"),
    box: one(".chat-composer-box"),
    field: one(".chat-composer-field"),
    send: one(".chat-composer-send button"),
    decision: one(".chat-decision"),
  };
}
