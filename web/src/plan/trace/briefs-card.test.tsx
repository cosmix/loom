import { cleanup, render, screen, within } from "@testing-library/react";
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
    expect(groups.map((group) => [group.folder, group.briefs.map((brief) => brief.name)])).toEqual([
      ["", ["common.md", "notes.txt"]],
      ["server", ["w1.md"]],
      ["web", ["c2.md", "c10.md"]],
    ]);
  });
});

describe("BriefsCard", () => {
  it("lists every brief by its mono name, grouped by stage folder, with a count", () => {
    mount(["common.md", "build/r1.md", "build/r2.md", "verify/v1.md"]);
    const card = screen.getByRole("region", { name: "Briefs" });
    expect(within(card).getByText("4 files in 2 stages")).toBeTruthy();
    const list = within(card).getByRole("list", { name: "Briefs" });
    const names = within(list)
      .getAllByRole("link")
      .map((link) => link.textContent);
    expect(names).toEqual(["common.md", "r1.md", "r2.md", "v1.md"]);
    expect(within(list).getAllByRole("link")[1]!.getAttribute("href")).toBe(
      "/f/demo/source/plans/demo/briefs/first/build/r1.md",
    );
    expect(list.querySelectorAll("code.trace-brief__name")).toHaveLength(4);
    // One folder heading per stage; the top-level files have none.
    const folders = [...card.querySelectorAll(".trace-briefs__folder")].map((el) => el.textContent);
    expect(folders).toEqual(["build", "verify"]);
  });

  it("counts files alone when the bundle has no stage folders", () => {
    mount(["common.md"]);
    expect(screen.getByText("1 file")).toBeTruthy();
  });

  it("draws nothing for a plan with no briefs", () => {
    mount([]);
    expect(screen.queryByRole("region", { name: "Briefs" })).toBeNull();
  });
});
