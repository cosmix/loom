import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import { checklistViewSchema, type ChecklistView } from "@/plan/api/schema";
import {
  clone,
  makeStubFetch,
  readingColumn,
  renderAt,
  type Store,
} from "@/plan/concept/concept-route-support";
import { projectAtom } from "@/plan/state/atoms";

// The panel's desktop region, without its socket and transcript, as the spine contracts stub it.
vi.mock("@/plan/chat/panel", async (original) => ({
  ...(await original<typeof import("@/plan/chat/panel")>()),
  ChatPanel: () => <aside aria-label="Chat" />,
  ChatRegistrations: () => null,
}));

const SPINE = "Feature documents";
const SECTION_NAMES = { "01": "Workflow model", "02": "Command surface", "03": "Review path" };

let pages: Record<string, unknown> = {};
const fetched: string[] = [];

function section(number: keyof typeof SECTION_NAMES, written: boolean) {
  return { number, name: SECTION_NAMES[number], written, open_questions: 0 };
}

function offer(kind: string, label: string) {
  return { id: kind, kind, label, target: null, feature: "atlas" };
}

function atlas(patch: Record<string, unknown>): ChecklistView {
  return checklistViewSchema.parse({
    ...checklistFixture.checklists[0]!,
    feature: "atlas",
    metaplans: [{ path: "doc/plans/atlas/index.md", title: "Atlas roadmap" }],
    ...patch,
  });
}

/** Every listed section written and every plan done: a feature with nothing left to start. */
function finishedChecklist(): ChecklistView {
  const plan = (slug: string) => ({
    slug,
    path: `doc/plans/atlas/DONE-PLAN-${slug}.md`,
    title: `Plan ${slug}`,
    state: "done",
    pressure_date: null,
    listed: true,
  });
  return atlas({
    sections: [section("01", true), section("02", true), section("03", true)],
    plan_rows: [plan("atlas-01"), plan("atlas-02")],
    plans: { listed: 2, total: 2, written: 2, running: 0, done: 2 },
    untested: 0,
    current: { kind: "ready_all_done" },
    steps: checklistFixture.checklists[0]!.steps.map((step) => ({ ...step, mark: "done" })),
    primary: null,
    secondary: [offer("plan_follow_ups", "Plan follow-ups")],
    items: [],
  });
}

const seed = (store: Store) =>
  store.set(projectAtom, { checklists: [finishedChecklist()], standalone: [], moves: [] });

/** The first sentence of the summary fixture, wherever its paragraphs sit. */
function firstSentence(): string {
  for (const block of summaryFixture.blocks) {
    const first = "sentences" in block ? block.sentences?.[0] : undefined;
    if (first) return first.text;
  }
  throw new Error("concept-summary.json holds no sentence");
}

beforeEach(() => {
  pages = {};
  fetched.length = 0;
  window.sessionStorage.clear();
  vi.stubGlobal(
    "fetch",
    vi.fn(makeStubFetch(() => ({ pages, docs: {}, failing: false }), fetched)),
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("the vision page shows the vision summary with Source one click away", async () => {
  pages = {
    "atlas/product/vision": {
      ...clone(summaryFixture),
      key: "atlas/product/vision",
      title: "Vision",
      feature: "atlas",
      path: "doc/product/atlas/index.md",
    },
  };
  renderAt("/f/atlas/vision", seed);
  const column = await readingColumn();
  const sentence = firstSentence();
  await waitFor(() => expect(column.textContent ?? "").toContain(sentence));
  const sources = await screen.findAllByRole("link", { name: "Source" });
  const hrefs = sources.map((link) => link.getAttribute("href") ?? "");
  expect(
    hrefs.some((href) => href.startsWith("/f/atlas/source/")),
    hrefs.join(", "),
  ).toBe(true);
});

it("a summary's Source link keeps the feature spine", async () => {
  pages = {
    "atlas/product/section_01": {
      ...clone(summaryFixture),
      key: "atlas/product/section_01",
      feature: "atlas",
      path: "doc/product/atlas/section_01.md",
    },
  };
  const { router } = renderAt("/f/atlas/product/section_01", seed);
  const [source] = await screen.findAllByRole("link", { name: "Source" });
  fireEvent.click(source!);
  await waitFor(() => expect(router.state.location.pathname).toMatch(/^\/f\/atlas\/source\//));
  expect(screen.getByRole("navigation", { name: SPINE })).toBeTruthy();
});
