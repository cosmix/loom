// Screenshot the planner's chat waiting on background tasks, in a real browser.
//
// Usage: node web/scripts/plan-chat-screens.mjs [--themes=<id>,<id>] [--browser=chromium|firefox]
//        <token-url> <out-dir> <route>...
//
// The arguments are plan-screens.mjs's, so scripts/plan-screens.sh runs this file around its
// server: scripts/plan-screens.sh --driver=web/scripts/plan-chat-screens.mjs <loom-binary>
// <out-dir> /files. A real server has no session to show, so the page's plan socket is played
// here (Playwright's routeWebSocket, as chat-layout-kit.mjs does) from src/plan/api/fixtures:
// hello, the chat snapshot with its session idle and one closed turn that launched the fixture's
// running agent, background.json and a commands frame of session-commands.json. Every REST route
// but the session's brief is the real server's, and hello names the fixture's project, so shoot a
// route that reads no feature of its own: /files.
//
// Each route is shot at 1440x900 and 390x844, in Ledger and Graphite unless --themes names others
// from THEMES, to <out-dir>/<route-slug>-chat-<shot>-<width>-<theme>.png. The shots, in order:
//   activity   the header's activity popover with the background row (the whole page)
//   idle-line  the chat panel, its head reading "Waiting on N background tasks"
//   open-list  the panel with the background list open
//   agent-row  the panel with the running Agent row, which stands outside the tool fold
//   slash      the panel with "/" typed and the command list open
// Exit 1 on a console error, a page error, an HTTP status >= 400 on a route, or a step whose
// element never shows; the shot is still taken, so the failure can be seen.

import { existsSync, mkdirSync, readdirSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { chromium, firefox } from "playwright-core";

const VIEWPORTS = [
  { width: 1440, height: 900 },
  { width: 390, height: 844 },
];

// THEMES, firefoxExecutable, BROWSERS, routeSlug, themeInitScript, watchErrors, bootstrap and
// takeFlags repeat plan-screens.mjs, which runs on import and exports nothing; keep them in step.
const THEMES = {
  ledger: { name: "ledger", scheme: "light", light: "ledger", dark: "gray" },
  harbor: { name: "harbor", scheme: "light", light: "harbor", dark: "gray" },
  aubergine: { name: "aubergine", scheme: "dark", light: "ledger", dark: "aubergine" },
  graphite: { name: "graphite", scheme: "dark", light: "ledger", dark: "gray" },
  midnight: { name: "midnight", scheme: "dark", light: "ledger", dark: "midnight" },
  obsidian: { name: "obsidian", scheme: "dark", light: "ledger", dark: "obsidian" },
  indigo: { name: "indigo", scheme: "dark", light: "ledger", dark: "indigo" },
};
const DEFAULT_THEMES = ["ledger", "graphite"];

const ROOT_TIMEOUT_MS = 15_000;
const STEP_TIMEOUT_MS = 10_000;
const SETTLE_MS = 400;
const PANEL = 'aside.chat-panel[aria-label="Chat"]';

function firefoxExecutable() {
  const pinned = firefox.executablePath();
  if (existsSync(pinned)) return pinned;
  const cache = process.env.PLAYWRIGHT_BROWSERS_PATH || join(homedir(), ".cache", "ms-playwright");
  if (!existsSync(cache)) return undefined;
  const builds = readdirSync(cache)
    .map((name) => /^firefox-(\d+)$/.exec(name))
    .filter((match) => match !== null)
    .sort((a, b) => Number(b[1]) - Number(a[1]));
  const found = builds.map((match) => join(cache, match[0], "firefox", "firefox"));
  return found.find((path) => existsSync(path));
}

const BROWSERS = {
  chromium: () => ({ type: chromium, options: { args: ["--no-sandbox"] } }),
  firefox: () => ({ type: firefox, options: { executablePath: firefoxExecutable() } }),
};

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
    localStorage.setItem("loom:light-variant", ${JSON.stringify(JSON.stringify(theme.light))});
    localStorage.setItem("loom:dark-variant", ${JSON.stringify(JSON.stringify(theme.dark))});
  `;
}

function watchErrors(page, errors) {
  page.on("console", (msg) => {
    if (msg.type() === "error") errors.push(`console: ${msg.text()}`);
  });
  page.on("pageerror", (err) => errors.push(`pageerror: ${err.message}`));
}

async function bootstrap(page, tokenUrl) {
  await page.goto(tokenUrl, { waitUntil: "load" });
  await page.waitForSelector("#root > *", { timeout: ROOT_TIMEOUT_MS });
  await page.evaluate(() => document.fonts.ready);
  return new URL(page.url()).origin;
}

function takeFlags(args) {
  const flags = {};
  while (args[0]?.startsWith("--")) {
    const [name, value = ""] = args.shift().slice(2).split(/=(.*)/s);
    flags[name] = value;
  }
  return flags;
}

// The played session ------------------------------------------------------------------------------

const fixture = (name) =>
  JSON.parse(
    readFileSync(new URL(`../src/plan/api/fixtures/${name}.json`, import.meta.url), "utf8"),
  );

/**
 * One closed turn: the user's message, two Read rows that fold, the done launch row of the
 * fixture's running agent (its `tool_use_id` is the task's, so the row reads the task's state),
 * and the reply. Each item is a fixture item with its text replaced, so it keeps the wire's keys.
 */
function agentTurn(snapshot, agent) {
  const [base] = snapshot.turns;
  const like = (type, fields) => ({ ...base.items.find((item) => item.type === type), ...fields });
  const tool = (id, fields) => like("tool", { id, status: "done", ...fields });
  const read = (n) =>
    tool(`read-${n}`, { tool_use_id: `toolu_read${n}`, summary: `Read doc/plans/p${n}.md` });
  const launch = tool("agent-1", {
    tool_use_id: agent.tool_use_id,
    name: "Agent",
    summary: agent.label,
    input: JSON.stringify({ description: agent.label, run_in_background: true }),
    result: "Async agent launched",
  });
  const text = "Survey the session bridge in the background while I read the plan.";
  const user = like("user", { id: "u1", text, attachments: [], origin: null, command: null });
  const markdown = "The survey runs in the background. I will report when it ends.";
  const items = [user, read(1), read(2), launch, like("text", { id: "a1", markdown })];
  return { ...base, id: "t1", open: false, changed_files: [], summaries_rebuilding: [], items };
}

/** What the socket plays and the label of the agent row the shots look for. */
function playedSession() {
  const hello = fixture("hello");
  const snapshot = fixture("chat-snapshot");
  const background = fixture("background");
  const agent = background.tasks.find((task) => task.kind === "agent" && task.state === "running");
  // Held by the viewer, with nothing read-only, so the composer takes the "/".
  const session = { ...snapshot.session, state: "idle", holder: hello.you.name, queued: 0 };
  const turns = [agentTurn(snapshot, agent)];
  const frames = [
    {
      ...hello,
      server: { ...hello.server, read_only: [] },
      presence: { entries: [] },
      authoring: { holder: { ...hello.you, away_since: null, frees_at: null } },
    },
    { type: "chat", frame: "snapshot", session, turns, queue: [], previous: null },
    background,
    { type: "commands", session: session.id, ...fixture("session-commands") },
  ];
  const brief = { ...fixture("session-brief"), session_id: session.id };
  return { frames, brief, agentLabel: agent.label };
}

/** Plays the plan socket and answers the one REST route that knows the session's id. */
async function playSession(context, played) {
  await context.routeWebSocket(
    (url) => url.pathname === "/ws",
    (socket) => {
      for (const frame of played.frames) socket.send(JSON.stringify(frame));
    },
  );
  await context.route(
    (url) => url.pathname === "/api/session/brief",
    (route) => route.fulfill({ json: played.brief }),
  );
}

// The shots ---------------------------------------------------------------------------------------

/** Runs one step; a failure is recorded and the shots go on, so the page it left can be seen. */
async function step(errors, label, run) {
  try {
    await run();
  } catch (error) {
    errors.push(`${label}: ${String(error.message).split("\n")[0]}`);
  }
}

async function open(page, url, errors) {
  const response = await page.goto(url, { waitUntil: "load" });
  if (response && response.status() >= 400) {
    errors.push(`navigation: ${url} answered ${response.status()}`);
  }
  await step(errors, `navigation: ${url} never rendered the app root`, () =>
    page.waitForSelector("#root > *", { timeout: ROOT_TIMEOUT_MS }),
  );
  await step(errors, `navigation: ${url} still loading`, () =>
    page.waitForFunction(
      () => !document.querySelector('[role="status"][aria-label^="Loading"]'),
      undefined,
      { timeout: ROOT_TIMEOUT_MS },
    ),
  );
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(SETTLE_MS);
}

/**
 * The header's activity popover, shot before the chat opens: below 1280px the chat is a modal
 * sheet over the header. The popover may already stand open on arrival (a rebuild question opens
 * it), so the pill is pressed only when it is closed, and Escape puts it away afterwards.
 */
async function shootActivity(page, file, errors) {
  const popover = page.locator('[aria-label="In progress"]');
  await step(errors, "the header's activity popover never opened", async () => {
    if (!(await popover.isVisible())) await page.locator(".activity-pill").click();
    await popover.waitFor({ timeout: STEP_TIMEOUT_MS });
  });
  await step(errors, "the activity popover lists no background row", () =>
    popover
      .getByText(/\d+ background tasks?/)
      .first()
      .waitFor({ timeout: STEP_TIMEOUT_MS }),
  );
  await page.waitForTimeout(SETTLE_MS);
  await page.screenshot({ path: file, fullPage: true });
  await page.keyboard.press("Escape");
  await step(errors, "the activity popover stayed open", () =>
    popover.waitFor({ state: "hidden", timeout: STEP_TIMEOUT_MS }),
  );
}

/** The chat panel: docked from 1280px, else the sheet the header's Chat button opens. */
async function openPanel(page, errors) {
  const panel = page.locator(PANEL);
  await step(errors, "the chat panel never showed", async () => {
    if (!(await panel.isVisible())) await page.locator("[data-chat-sheet-trigger]").click();
    await panel.waitFor({ timeout: STEP_TIMEOUT_MS });
  });
  return panel;
}

/** The four panel shots, in the order the header comment lists them. */
async function shootPanel(page, panel, agentLabel, fileFor, errors) {
  const wait = { timeout: STEP_TIMEOUT_MS };
  const shot = async (name) => {
    await page.waitForTimeout(SETTLE_MS);
    // A panel that never showed has no box to shoot; the page says why.
    if (await panel.isVisible()) await panel.screenshot({ path: fileFor(name) });
    else await page.screenshot({ path: fileFor(name) });
  };
  const disclosure = panel.getByRole("button", { name: "Background tasks", exact: true });

  await step(errors, "the idle line never showed", () =>
    panel.locator(".chat-head-sub .chat-background-line").waitFor(wait),
  );
  await shot("idle-line");

  await step(errors, "the background list never opened", async () => {
    await disclosure.click(wait);
    await panel.locator(".chat-background-list li").first().waitFor(wait);
  });
  await shot("open-list");
  // Pressed again: the script shoots the disclosure's own toggle, not its Escape key.
  await step(errors, "the background list never closed", () => disclosure.click(wait));

  const row = panel.getByRole("button", { name: new RegExp(agentLabel) }).first();
  await step(errors, "the running Agent row is not in view (inside the tool fold?)", async () => {
    await row.waitFor(wait);
    await row.scrollIntoViewIfNeeded(wait);
    const folded = await row.evaluate((node) => node.closest("details.chat-tool-fold") !== null);
    if (folded) throw new Error("the row is inside details.chat-tool-fold");
  });
  await shot("agent-row");

  await step(errors, 'the command list never opened on "/"', async () => {
    await panel.getByRole("textbox", { name: "Message" }).click(wait);
    await page.keyboard.type("/");
    await page.getByRole("listbox", { name: "Commands" }).waitFor(wait);
  });
  await shot("slash");
}

async function shootTheme(browser, theme, viewport, ctx) {
  const context = await browser.newContext({ viewport });
  await context.addInitScript(themeInitScript(theme));
  await playSession(context, ctx.played);
  const page = await context.newPage();
  const errors = [];
  watchErrors(page, errors);
  const origin = await bootstrap(page, ctx.tokenUrl);
  for (const route of ctx.routes) {
    const fileFor = (shot) =>
      join(ctx.outDir, `${routeSlug(route)}-chat-${shot}-${viewport.width}-${theme.name}.png`);
    const before = errors.length;
    await page.setViewportSize(viewport);
    await open(page, origin + route, errors);
    await shootActivity(page, fileFor("activity"), errors);
    const panel = await openPanel(page, errors);
    await shootPanel(page, panel, ctx.played.agentLabel, fileFor, errors);
    console.log(`${errors.length === before ? "ok  " : "FAIL"} ${fileFor("*")}`);
  }
  await context.close();
  return errors;
}

async function main() {
  const args = process.argv.slice(2);
  const flags = takeFlags(args);
  const themes = (flags.themes ? flags.themes.split(",") : DEFAULT_THEMES).map((id) =>
    Object.hasOwn(THEMES, id) ? THEMES[id] : undefined,
  );
  const browserName = flags.browser ?? "chromium";
  const makeEngine = Object.hasOwn(BROWSERS, browserName) ? BROWSERS[browserName] : undefined;
  const unknown = Object.keys(flags).filter((name) => name !== "themes" && name !== "browser");
  const [tokenUrl, outDir, ...routes] = args;
  if (
    unknown.length > 0 ||
    makeEngine === undefined ||
    themes.includes(undefined) ||
    !tokenUrl ||
    !outDir ||
    routes.length === 0
  ) {
    console.error(
      `usage: node web/scripts/plan-chat-screens.mjs [--themes=<id>,<id>] [--browser=chromium|firefox] <token-url> <out-dir> <route>...; themes: ${Object.keys(THEMES).join(", ")}`,
    );
    process.exit(2);
  }
  mkdirSync(outDir, { recursive: true });
  const ctx = { tokenUrl, outDir, routes, played: playedSession() };
  const engine = makeEngine();
  const browser = await engine.type.launch(engine.options);
  const failures = [];
  try {
    for (const theme of themes) {
      for (const viewport of VIEWPORTS) {
        const errors = await shootTheme(browser, theme, viewport, ctx);
        for (const e of errors) failures.push(`[${theme.name} ${viewport.width}] ${e}`);
      }
    }
  } finally {
    await browser.close();
  }
  for (const f of failures) console.error(f);
  process.exit(failures.length === 0 ? 0 : 1);
}

await main();
