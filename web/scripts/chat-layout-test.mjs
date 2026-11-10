// Run with: bun scripts/chat-layout-test.mjs [--shots=<dir>]
//
// The chat column in short windows, measured in a real browser: its top, its transcript and its
// composer never share a pixel, none leaves the column, the transcript keeps its floor and takes
// the scroll, and the composer's box and Send stay in view (src/plan/chat/chat-column.css).
// jsdom lays nothing out, so no vitest file can see any of this.
// `--shots=<dir>` also saves one screenshot per case.

import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import { join } from "node:path";

import {
  KICKOFF,
  launchBrowser,
  measure,
  openChat,
  settle,
  startPlanner,
} from "./chat-layout-kit.mjs";

// The panel docks from 1280px; under it the chat is a sheet as tall as the window.
const PANEL_VIEWPORTS = [
  [1440, 900],
  [1280, 500],
  [1280, 420],
  [1280, 360],
  [1280, 330],
  [900, 360],
  [390, 600],
  [390, 420],
];
const CASES = [
  ...["long", "decision", "empty"].flatMap((scenario) =>
    PANEL_VIEWPORTS.map(([width, height]) => ({ scenario, width, height, route: "/files" })),
  ),
  { scenario: "long", width: 1280, height: 500, route: KICKOFF },
  { scenario: "long", width: 1280, height: 360, route: KICKOFF },
  { scenario: "long", width: 900, height: 420, route: KICKOFF },
];
// The transcript's floor, and the column height from which the box and Send are in view with no
// scrolling: 15rem, or 19rem with the queue strip above the box (chat-column.css). In a shorter
// column the composer scrolls to them. Rounding and sub-pixel edges get one pixel.
const FLOOR_PX = 112;
const FLOOR_SHARE = 0.3;
const IN_VIEW_FROM_PX = { long: 240, empty: 240, decision: 304 };
const SLACK = 1;

const shots = process.argv.find((arg) => arg.startsWith("--shots="))?.slice("--shots=".length);
if (shots) mkdirSync(shots, { recursive: true });

const within = (inner, outer) =>
  inner.top >= outer.top - SLACK && inner.bottom <= outer.bottom + SLACK;

/** Every rule the column keeps, as [what, holds] pairs for one measured page. */
function rules(m, { scenario, route }) {
  const panel = route !== KICKOFF;
  const floor = Math.min(FLOOR_PX, FLOOR_SHARE * m.column.height);
  const list = [
    ["the top ends where the transcript starts", m.head.bottom <= m.body.top + SLACK],
    ["the transcript ends where the composer starts", m.body.bottom <= m.composer.top + SLACK],
    ["the top stays in the column", within(m.head, m.column)],
    ["the transcript stays in the column", within(m.body, m.column)],
    ["the composer stays in the column", within(m.composer, m.column)],
    ["the page does not scroll", m.page.scrollHeight <= m.viewport.height],
    ["the transcript keeps its floor", m.body.height >= floor - SLACK],
    ["the text field stays in the box", within(m.field, m.box)],
  ];
  if (panel)
    list.push(["the column fits the window", m.column.bottom <= m.viewport.height + SLACK]);
  if (m.log) {
    list.push(
      ["the log stays in its frame", within(m.log, m.body)],
      ["the transcript takes the scroll", m.log.scrollHeight > m.log.clientHeight],
      [
        "the transcript follows its end",
        m.log.scrollTop + m.log.clientHeight >= m.log.scrollHeight - 2,
      ],
    );
  }
  if (scenario === "decision") {
    list.push(["the pending decision ends in view", m.decision.bottom <= m.log.bottom + SLACK]);
  }
  if (m.column.height >= IN_VIEW_FROM_PX[scenario]) {
    list.push(
      ["the box is in view", within(m.box, m.composer)],
      ["Send is in view", within(m.send, m.box) && within(m.send, m.composer)],
    );
  }
  return list;
}

/** A reader who wheeled back into the transcript is left there when the window changes height. */
async function readerStaysPut() {
  const chat = await openChat(browser, planner.origin, {
    viewport: { width: 1280, height: 800 },
    scenario: "long",
  });
  const log = chat.page.locator(".chat-transcript");
  await log.hover();
  await chat.page.mouse.wheel(0, -900);
  await chat.page.waitForFunction(
    (node) => node.scrollHeight - node.clientHeight - node.scrollTop > 600,
    await log.elementHandle(),
  );
  const before = await log.evaluate((node) => node.scrollTop);
  await chat.page.setViewportSize({ width: 1280, height: 420 });
  await chat.page.waitForFunction(() => innerHeight === 420);
  await settle(chat.page);
  checked += 1;
  const after = await log.evaluate((node) => node.scrollTop);
  if (after !== before) failures.push(`reader scrolled to ${before}, moved to ${after} on resize`);
  await chat.close();
}

const planner = await startPlanner();
const browser = await launchBrowser();
const failures = [];
let checked = 0;

async function check(label, chat, options) {
  const measured = await chat.page.evaluate(measure);
  for (const error of chat.errors) failures.push(`${label}: page error: ${error}`);
  await chat.page.locator(".chat-composer-send button").scrollIntoViewIfNeeded();
  const scrolled = await chat.page.evaluate(measure);
  const reached =
    within(scrolled.send, scrolled.composer) && within(scrolled.send, scrolled.column);
  for (const [what, holds] of [...rules(measured, options), ["Send can be scrolled to", reached]]) {
    checked += 1;
    if (!holds) failures.push(`${label}: ${what} ${JSON.stringify(measured)}`);
  }
  if (shots) await chat.page.screenshot({ path: join(shots, `${label.replaceAll(" ", "-")}.png`) });
}

try {
  for (const { scenario, width, height, route } of CASES) {
    const where = route === KICKOFF ? "kickoff" : "panel";
    const chat = await openChat(browser, planner.origin, {
      viewport: { width, height },
      scenario,
      route,
    });
    await check(`${where} ${scenario} ${width}x${height}`, chat, { scenario, route });
    await chat.close();
  }
  // A window made shorter while a decision waits: the transcript gives up height with no scroll
  // event, and must still end on the decision.
  const chat = await openChat(browser, planner.origin, {
    viewport: { width: 1280, height: 800 },
    scenario: "decision",
  });
  await chat.page.setViewportSize({ width: 1280, height: 420 });
  await chat.page.waitForFunction(() => innerHeight === 420);
  await settle(chat.page);
  await check("panel decision 1280x800 to 1280x420", chat, {
    scenario: "decision",
    route: "/files",
  });
  await chat.close();
  await readerStaysPut();
} finally {
  await browser.close();
  await planner.close();
}

assert.equal(failures.length, 0, `chat layout:\n${failures.join("\n")}`);
console.log(`chat layout holds: ${checked} checks in ${CASES.length + 2} windows`);
