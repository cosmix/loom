import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import traceFixture from "@/plan/api/fixtures/trace.json";
import { traceViewSchema, type TraceView } from "@/plan/api/schema-trace";

import { briefGroups } from "./brief-groups";
import { BriefsCard } from "./briefs-card";
import { bundleLinks } from "./model";
import { traceAtom } from "./state";

const PLAN = "doc/plans/demo/PLAN-first.md";
const HOME = "doc/plans/demo/briefs/first";

function traceWith(files: string[]): TraceView {
  const trace = traceViewSchema.parse(structuredClone(traceFixture));
  trace.documents.find((doc) => doc.path === PLAN)!.bundle = files.map((file) => `${HOME}/${file}`);
  return trace;
}

let served: TraceView;

beforeEach(() => {
  // The card reads the trace the store holds; its refetch answers with the same trace.
  vi.stubGlobal("fetch", () => Promise.resolve(new Response(JSON.stringify(served))));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function mount(files: string[]) {
  const store = createStore();
  initClient(store);
  served = traceWith(files);
  store.set(traceAtom("demo"), { trace: served, error: null });
  render(
    <Provider store={store}>
      <MemoryRouter>
        <BriefsCard feature="demo" path={PLAN} />
      </MemoryRouter>
    </Provider>,
  );
}

describe("briefGroups", () => {
  it("puts the bundle's own files first, then each folder, both in natural order", () => {
    const files = ["web/c10.md", "web/c2.md", "server/w1.md", "common.md", "notes.txt"];
    const groups = briefGroups(bundleLinks(traceWith(files), PLAN, "demo"));
    expect(groups.map((group) => [group.folder, group.briefs.map((brief) => brief.stem)])).toEqual([
      ["", ["common", "notes.txt"]],
      ["server", ["w1"]],
      ["web", ["c2", "c10"]],
    ]);
  });
});

describe("BriefsCard", () => {
  it("draws a short bundle open, grouped by folder, with its count and no toggle", () => {
    mount(["common.md", "build/r1.md", "build/r2.md"]);
    const card = screen.getByRole("region", { name: "Briefs" });
    expect(within(card).getByText("3 briefs")).toBeTruthy();
    expect(within(card).queryByRole("button")).toBeNull();
    const list = within(card).getByRole("list", { name: "Briefs" });
    const names = within(list)
      .getAllByRole("link")
      .map((link) => link.textContent);
    expect(names).toEqual(["common.md", "build/r1.md", "build/r2.md"]);
    // A chip shows the stem; the folder and the extension are read, not shown.
    expect(within(list).getByRole("link", { name: "build/r1.md" }).getAttribute("title")).toBe(
      "build/r1.md",
    );
    // One folder label, with its count; the top-level files have none.
    const folders = [...card.querySelectorAll(".trace-briefs__folder")].map((el) => el.textContent);
    expect(folders).toEqual(["build2"]);
  });

  it("folds a bundle of more than six behind its count, naming its folders, and opens in place", () => {
    const files = Array.from({ length: 7 }, (_, index) => `chat-web/c${index + 1}.md`);
    mount([...files, "common.md"]);
    const card = screen.getByRole("region", { name: "Briefs" });
    const toggle = within(card).getByRole("button", { name: "8 briefs" });
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(within(card).queryByRole("list", { name: "Briefs" })).toBeNull();
    expect(card.textContent).toContain("common · chat-web 7");

    fireEvent.click(toggle);
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    const list = within(card).getByRole("list", { name: "Briefs" });
    expect(toggle.getAttribute("aria-controls")).toBe(list.id);
    expect(within(list).getAllByRole("link")).toHaveLength(8);
  });

  it("draws nothing for a plan with no briefs", () => {
    mount([]);
    expect(screen.queryByRole("region", { name: "Briefs" })).toBeNull();
  });
});
