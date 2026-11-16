// A product section's page and a plan's page in a real browser with no plan server behind them,
// in every state their headers draw at once: a document changed since it was settled, a plan whose
// spec moved and that was never pressure-tested, a section built by several plans, a plan that
// implements several sections, and a plan with a brief bundle.
//
// Usage:
//   bun scripts/doc-header-review.mjs serve [port]     browse the two pages (default port 7290)
//   bun scripts/doc-header-review.mjs shots <out-dir>  screenshot them, light and dark, 1440 and 390
//
// Vite serves `web/src`; one front server answers `/api` and `/ws` from `src/plan/api/fixtures`
// and passes every other request to Vite, so the page and its API share an origin. Exit 1 from
// `shots` on a page error or an API route the mock does not answer.

import { mkdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { launchBrowser, settle, startPlanner } from "./chat-layout-kit.mjs";

const fixture = (name) =>
  JSON.parse(
    readFileSync(new URL(`../src/plan/api/fixtures/${name}.json`, import.meta.url), "utf8"),
  );

const FEATURE = "demo";
const SECTION = "doc/product/demo/section_02.md";
const PLAN = "doc/plans/demo/PLAN-demo-04-settle-ledger.md";
const PLAN_SLUG = "demo-04-settle-ledger";
const METAPLAN = "doc/plans/demo/index.md";
const ENTRY = "4-settle-ledger-plan-demo-04-settle-ledgermd";
const COMMIT = "0a1b2c3d4e5f60718293a4b5c6d7e8f901234567";

export const ROUTES = {
  spec: `/f/${FEATURE}/product/section_02`,
  plan: `/f/${FEATURE}/engineering/${PLAN_SLUG}`,
};

const BRIEFS = [
  "common.md",
  "contracts/c1-wire.md",
  "contracts/c2-settle-line.md",
  "build/b1-ledger.md",
  "build/b2-heading.md",
  "build/b3-confirm.md",
  "verify/v1-journeys.md",
].map((name) => `doc/plans/demo/briefs/${PLAN_SLUG}/${name}`);

const SECTION_TEXT = [
  "# 02 Beta",
  "",
  "## Three",
  "",
  "Text three. The bridge keeps one session per project and replays its transcript to every tab.",
  "",
  "## Four",
  "",
  "Text four. A settle writes one line and commits it on its own.",
  "",
  "Settled: body=sha256:abc",
].join("\n");

const METAPLAN_TEXT = [
  "# Engineering",
  "",
  "## 4. Settle ledger (`PLAN-demo-04-settle-ledger.md`)",
  "",
  "Record each settle as one committed line, and show what moved since on every page that reads it.",
  "",
  "Implements: section_01, section_02",
].join("\n");

const VISION_TEXT = [
  "# Demo",
  "",
  "## Specification sections",
  "",
  "| # | Section | Covers |",
  "| --- | --- | --- |",
  "| 01 | Alpha | How a plan starts from a statement of intent. |",
  "| 02 | Beta | How a settled document is recorded, and what the reader sees when it moves. |",
].join("\n");

const DOCS = {
  [SECTION]: { title: "02 Beta", text: SECTION_TEXT, kind: "section" },
  [METAPLAN]: {
    title: "Engineering",
    text: METAPLAN_TEXT,
    kind: "metaplan",
    sections: [{ level: 2, title: "4. Settle ledger", anchor: ENTRY, start_line: 3, end_line: 7 }],
  },
  "doc/product/demo/index.md": { title: "Demo", text: VISION_TEXT, kind: "vision" },
  [PLAN]: {
    ...fixture("files-doc-plan"),
    title: "Settle ledger",
    plan: { ...fixture("files-doc-plan").plan, slug: PLAN_SLUG, pressure_date: null },
  },
};

/** The trace with the section revised and one settled plan, with briefs, whose spec moved. */
function trace() {
  const view = fixture("trace");
  const first = view.documents.find((doc) => doc.path.endsWith("PLAN-first.md"));
  view.documents.push({
    ...first,
    path: PLAN,
    title: "Settle ledger",
    slug: PLAN_SLUG,
    settle: "upstream_changed",
    stub: { path: METAPLAN, anchor: ENTRY },
    bundle: BRIEFS,
  });
  const cover = (stage) => ({
    kind: stage ? "stage" : "plan",
    path: PLAN,
    anchor: stage ? `stage:${stage}` : null,
    slug: PLAN_SLUG,
    stage,
  });
  view.headings.forEach((heading, index) =>
    heading.covered_by.push(cover(index % 2 ? "build" : null)),
  );
  return view;
}

const sentence = (id, text, anchor) => ({
  id,
  text,
  marks: [],
  citations: [{ kind: "source", path: SECTION, anchor }],
  rests_on: [{ path: SECTION, anchor }],
  plans: [],
});

const BLOCKS = [
  { kind: "heading", level: 2, text: "What a settle records" },
  {
    kind: "paragraph",
    sentences: [
      sentence("s1", "Settling a document writes one line that names its text.", "three"),
      sentence("s2", "The line is committed on its own, apart from any other change.", "four"),
    ],
  },
];

function page(key, path, title, extra) {
  return {
    ...fixture("concept-summary"),
    key,
    path,
    title,
    feature: FEATURE,
    key_chips: [],
    blocks: BLOCKS,
    implemented_by: [],
    ...extra,
  };
}

const sectionLink = (number, name) => ({
  number,
  name,
  doc: {
    id: `section_0${number}`,
    title: name,
    path: `doc/product/demo/section_0${number}.md`,
    page: `demo/product/section_0${number}`,
  },
});

const PAGES = {
  "demo/product/section_02": () =>
    page("demo/product/section_02", SECTION, "02 Beta", {
      implemented_by: [
        { slug: "demo-01-wire", title: "Wire and schema", state: "done" },
        { slug: "demo-03-heading", title: "Document heading", state: "running" },
        { slug: PLAN_SLUG, title: "Settle ledger", state: "written" },
        { slug: "demo-07-follow-ups", title: "Follow-ups", state: "retired" },
      ],
    }),
  [`demo/engineering/${PLAN_SLUG}`]: () =>
    page(`demo/engineering/${PLAN_SLUG}`, PLAN, "Settle ledger", {
      plan_state: "written",
      last_pressure: null,
      state: { state: "current", out_of_date: null },
      implements: {
        kind: "sections",
        links: [sectionLink(1, "Alpha"), sectionLink(2, "Beta"), sectionLink(3, "Gamma")],
      },
    }),
};

function doc(url) {
  const path = `doc/${url.searchParams.get("path")}`;
  const known = DOCS[path];
  if (!known) return null;
  return { ...fixture("files-doc"), path, sections: [], feature: FEATURE, ...known };
}

function hello() {
  const frame = fixture("hello");
  return {
    ...frame,
    server: { ...frame.server, read_only: [] },
    presence: { entries: [] },
    authoring: { holder: { ...frame.you, away_since: null, frees_at: null } },
  };
}

/** The answer of an API route, or null for one the mock does not know. */
function answer(url) {
  const { pathname } = url;
  if (pathname.startsWith("/api/concept/")) {
    return PAGES[decodeURIComponent(pathname.slice("/api/concept/".length))]?.() ?? null;
  }
  switch (pathname) {
    case "/api/server":
      return { ...fixture("server-info"), read_only: [] };
    case "/api/tree":
      return fixture("files-tree");
    case `/api/trace/${FEATURE}`:
      return trace();
    case "/api/doc":
      return doc(url);
    case "/api/settled-version":
      return { commit: COMMIT };
    case "/api/run/state":
      return fixture("run-state");
    case "/api/pressure/options":
      return fixture("pressure-options");
    case "/api/summaries/panel":
      return fixture("summary-panel");
    case "/api/session/commands":
      return fixture("session-commands");
    case "/api/open-questions":
      return { counts: { [SECTION]: 2 } };
    case "/api/hashes":
      return { head: COMMIT, docs: {} };
    case "/api/built":
      return { notes: [] };
    default:
      return null;
  }
}

/** Whether the request is for a page of the app, which Vite would answer with the dashboard. */
function isPage(request, url) {
  return (
    (request.headers.get("sec-fetch-dest") === "document" ||
      (request.headers.get("accept") ?? "").startsWith("text/html")) &&
    !url.pathname.startsWith("/@") &&
    !url.pathname.startsWith("/src/")
  );
}

/** The front server: fixtures for `/api` and `/ws`, Vite for the rest. */
export function serve(viteOrigin, port, unanswered = new Set()) {
  return Bun.serve({
    port,
    hostname: "127.0.0.1",
    async fetch(request, server) {
      const url = new URL(request.url);
      if (url.pathname === "/ws") {
        return server.upgrade(request) ? undefined : new Response(null, { status: 400 });
      }
      if (url.pathname.startsWith("/api/")) {
        const body = answer(url);
        if (body === null) unanswered.add(url.pathname);
        return Response.json(body ?? { error: "not_found", message: "not mocked" }, {
          status: body === null ? 404 : 200,
        });
      }
      const target = isPage(request, url) ? "/plan.html" : url.pathname + url.search;
      const reply = await fetch(viteOrigin + target, {
        headers: { accept: request.headers.get("accept") ?? "*/*" },
      });
      return new Response(reply.body, { status: reply.status, headers: reply.headers });
    },
    websocket: {
      open: (socket) => void socket.send(JSON.stringify(hello())),
      message: () => {},
    },
  });
}

const VIEWPORTS = [
  { name: "wide", width: 1440, height: 1500 },
  { name: "narrow", width: 390, height: 2200 },
];
const SCHEMES = ["light", "dark"];

async function shoot(browser, origin, outDir, { name, route, viewport, scheme }) {
  const context = await browser.newContext({
    viewport,
    reducedMotion: "reduce",
    colorScheme: scheme,
  });
  await context.addInitScript(
    (value) => localStorage.setItem("loom:theme", JSON.stringify(value)),
    scheme,
  );
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(origin + route, { waitUntil: "load" });
  await page.waitForSelector(".doc-heading .trace-settle-button");
  await page.waitForSelector(".trace-changes:not([aria-busy])");
  await page.evaluate(() => document.fonts.ready);
  await settle(page);
  const file = join(outDir, `${name}-${viewport.name}-${scheme}.png`);
  await page.screenshot({ path: file });
  await context.close();
  return { file, errors };
}

async function shots(origin, outDir, unanswered) {
  mkdirSync(outDir, { recursive: true });
  const browser = await launchBrowser();
  let failed = false;
  try {
    for (const [name, route] of Object.entries(ROUTES)) {
      for (const viewport of VIEWPORTS) {
        for (const scheme of SCHEMES) {
          const shot = await shoot(browser, origin, outDir, { name, route, viewport, scheme });
          console.log(shot.file);
          for (const error of shot.errors) console.error(`  page error: ${error}`);
          failed ||= shot.errors.length > 0;
        }
      }
    }
  } finally {
    await browser.close();
  }
  for (const path of unanswered) console.error(`not mocked: ${path}`);
  return failed;
}

async function main() {
  const [command, argument] = process.argv.slice(2);
  if (command !== "serve" && !(command === "shots" && argument)) {
    throw new Error("usage: doc-header-review.mjs serve [port] | shots <out-dir>");
  }
  const planner = await startPlanner();
  const unanswered = new Set();
  const front = serve(
    planner.origin,
    command === "serve" ? Number(argument ?? 7290) : 0,
    unanswered,
  );
  const origin = `http://127.0.0.1:${front.port}`;
  if (command === "serve") {
    console.log(`Spec page: ${origin}${ROUTES.spec}`);
    console.log(`Plan page: ${origin}${ROUTES.plan}`);
    console.log("Ctrl-C stops the server.");
    return;
  }
  const failed = await shots(origin, argument, unanswered);
  front.stop(true);
  await planner.close();
  process.exit(failed ? 1 : 0);
}

await main();
