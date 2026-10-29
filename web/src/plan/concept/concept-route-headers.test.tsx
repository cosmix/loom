import { cleanup, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";

import { clone, makeStubFetch, planPage, renderAt, text, type Page } from "./concept-route-support";

const extras = vi.hoisted(() => ({
  header: vi.fn(() => null),
  chip: vi.fn(() => null),
  row: vi.fn(() => null),
}));
vi.mock("./page-header-extras", () => ({ PageHeaderExtras: extras.header }));
vi.mock("./key-chip-extras", () => ({ KeyChipExtras: extras.chip }));
vi.mock("./key-row-extras", () => ({ KeyRowExtras: extras.row }));

let pages: Record<string, unknown> = {};
let docs: Record<string, unknown> = {};
let failing = false;
const fetched: string[] = [];

function serveOverview(overview: Page = clone(overviewFixture)): void {
  pages = {
    "overview/demo/product": { ...overview, key: "overview/demo/product" },
    "overview/demo/engineering": { ...overview, key: "overview/demo/engineering" },
    "demo/product/section_01": clone(summaryFixture),
    "demo/product/section_02": {
      ...clone(summaryFixture),
      key: "demo/product/section_02",
      title: "Review",
    },
  };
}
beforeEach(() => {
  pages = {};
  docs = {};
  failing = false;
  fetched.length = 0;
  extras.header.mockClear();
  extras.chip.mockClear();
  extras.row.mockClear();
  window.sessionStorage.clear();
  vi.stubGlobal("fetch", vi.fn(makeStubFetch(() => ({ pages, docs, failing }), fetched)));
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
describe("summary page header", () => {
  it("header shows title, path, colour, state and source view", async () => {
    serveOverview();
    renderAt("/f/demo/product/section_01");
    const title = await screen.findByRole("heading", { level: 1, name: "Planning" });
    const header = title.closest("header") as HTMLElement;
    expect(within(header).getByText("doc/product/demo/section_01.md")).toBeTruthy();
    expect((title.previousElementSibling as HTMLElement).style.color).toBe("var(--doc-2)");
    expect(within(header).getByText("Out of date: the source changed")).toBeTruthy();
    const link = within(header).getByRole("link", { name: "Source" });
    expect(link.getAttribute("href")).toBe("/files/product/demo/section_01.md");
    expect(extras.header).toHaveBeenCalled();
  });
  it("plan header shows state and pressure date", async () => {
    pages = {
      "demo/engineering/a": planPage({ key: "demo/engineering/a", last_pressure: "2026-10-20" }),
    };
    renderAt("/f/demo/engineering/a");
    await screen.findByRole("heading", { level: 1, name: "Plan a" });
    expect(screen.getByTitle("Plan state: written").textContent).toBe("written");
    expect(text()).toContain("2026-10-20");
  });
  it("locked notice on a done plan", async () => {
    pages = { "demo/engineering/a": planPage({ key: "demo/engineering/a", plan_state: "done" }) };
    renderAt("/f/demo/engineering/a");
    const note = await screen.findByRole("note");
    expect(note.textContent).toContain("This plan is done. Record changes in a new plan.");
  });
  it("locked notice on a running plan", async () => {
    pages = {
      "demo/engineering/a": planPage({ key: "demo/engineering/a", plan_state: "running" }),
    };
    renderAt("/f/demo/engineering/a");
    const note = await screen.findByRole("note");
    expect(note.textContent).toContain(
      "This plan is running. loom run works from the committed copy.",
    );
    expect(within(note).getByText("loom run", { selector: "code" })).toBeTruthy();
  });
  it("Implemented by with states", async () => {
    serveOverview();
    renderAt("/f/demo/product/section_01");
    await screen.findByRole("heading", { level: 1, name: "Planning" });
    expect(text()).toContain("Implemented by:");
    expect(
      screen.getByRole("link", { name: "Concept mode (in progress)" }).getAttribute("href"),
    ).toBe("/f/demo/engineering/feat-planner-05-concept-mode");
    expect(text()).toContain("(in progress)");
  });
});
