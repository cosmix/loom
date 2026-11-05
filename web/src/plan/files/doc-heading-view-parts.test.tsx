import { cleanup, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import type { ConceptPage } from "@/plan/api/schema-concept";
import type { PlanView } from "@/plan/api/schema-files";
import { openQuestionsAtom } from "@/plan/review/live";
import { projectAtom } from "@/plan/state/atoms";

import { authorStore, makeDoc, mount, withFeature } from "./doc-heading-kit";

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date(2026, 9, 29, 0, 15));
  window.localStorage.clear();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe("FileDocHeading chips and crumbs", () => {
  it("names the feature by its slug before the checklist arrives", () => {
    mount(makeDoc({ feature: "feat-planner" }));
    const nav = screen.getByRole("navigation", { name: "Breadcrumb" });

    expect(within(nav).getAllByRole("listitem")[0]?.textContent).toBe("feat-planner");
  });

  it("reads a section as its number of the feature's total", () => {
    const store = withFeature(authorStore(), "demo");
    const sections = ["01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12"];
    store.set(projectAtom, (project) => ({
      ...project!,
      checklists: [
        {
          ...project!.checklists[0]!,
          sections: sections.map((number) => ({
            number,
            name: `S${number}`,
            written: true,
            open_questions: 0,
          })),
        },
      ],
    }));
    mount(makeDoc({ kind: "section", plan: null, path: "doc/product/demo/section_05.md" }), store);
    const nav = screen.getByRole("navigation", { name: "Breadcrumb" });

    expect(
      within(nav)
        .getAllByRole("listitem")
        .map((item) => item.textContent),
    ).toEqual(["demo", "Product spec", "Section 05 of 12"]);
  });

  it.each<[PlanView["state"], string, string]>([
    ["running", "accent", ".aurora-roundel"],
    ["done", "ok", "svg.lucide-circle-check-big"],
    ["written", "neutral", "svg.lucide-pencil-line"],
  ])("tones the %s state chip %s", (state, tone, glyph) => {
    mount(makeDoc({}, { state }));

    const chip = screen.getByText(state[0]!.toUpperCase() + state.slice(1)).closest("[data-chip]");
    expect(chip?.getAttribute("data-tone")).toBe(tone);
    expect(chip?.querySelector(glyph)).not.toBeNull();
    expect(chip?.querySelector(".activity-glyph") !== null).toBe(state === "running");
  });

  it("offers nothing for a done plan with no follow-up offer, not dead buttons", () => {
    mount(makeDoc({ editable: false }, { state: "done" }));

    expect(screen.queryByText("No offer is available")).toBeNull();
    expect(screen.queryByRole("button", { name: "Plan follow-ups" })).toBeNull();
  });

  it("chips a section's open questions as a warning link to them", () => {
    const store = authorStore();
    store.set(openQuestionsAtom, { "doc/product/demo/section_05.md": 2 });
    mount(makeDoc({ kind: "section", plan: null, path: "doc/product/demo/section_05.md" }), store);

    const chip = screen.getByRole("link", { name: "2 open questions" });
    expect(chip.getAttribute("href")).toBe("/files/product/demo/section_05.md#open-questions");
    expect(chip.getAttribute("data-tone")).toBe("warning");
  });

  it.each<[string, ConceptPage["state"], string, string]>([
    ["current", { state: "current", out_of_date: null }, "Summary current", "ok"],
    ["rebuilding", { state: "rebuilding", out_of_date: null }, "Summary rebuilding", "accent"],
    [
      "out of date",
      { state: "current", out_of_date: { reason: "source_changed", detail: null } },
      "Out of date: the source changed",
      "warning",
    ],
  ])(
    "chips a section's %s summary, questions, builders and change time",
    async (_, state, label, tone) => {
      const page = { ...structuredClone(summaryFixture), state, feature: "demo" } as ConceptPage;
      page.implemented_by = [
        { slug: "feat-planner-05-concept-mode", title: "Concept mode", state: "in_progress" },
        { slug: "feat-planner-09-reading", title: "Reading", state: "done" },
      ];
      vi.spyOn(globalThis, "fetch").mockImplementation(() =>
        Promise.resolve(new Response(JSON.stringify(page))),
      );
      const store = authorStore();
      store.set(openQuestionsAtom, { "doc/product/demo/section_05.md": 2 });
      const doc = makeDoc({
        kind: "section",
        plan: null,
        path: "doc/product/demo/section_05.md",
        last_change: new Date(2026, 9, 26, 0, 15).toISOString(),
      });
      mount(doc, store);

      await screen.findByText(label);
      const chips = [...document.querySelectorAll("[data-chip]")];
      expect(chips.map((chip) => chip.textContent)).toEqual([
        label,
        "2 open questions",
        "Built by 05 (Concept mode, in progress) · 09 (Reading, done)",
        "Changed 3 d ago",
      ]);
      expect(chips[0]?.getAttribute("data-tone")).toBe(tone);
      expect(chips[1]?.getAttribute("data-tone")).toBe("warning");
      expect(chips[3]?.querySelector("svg.lucide-clock")).not.toBeNull();
    },
  );

  it("leaves a section without a loaded summary to its questions and change time", () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response("{}", { status: 404 }));
    mount(
      makeDoc({
        kind: "section",
        plan: null,
        path: "doc/product/demo/section_06.md",
        last_change: new Date(2026, 9, 28, 0, 15).toISOString(),
      }),
    );

    expect([...document.querySelectorAll("[data-chip]")].map((chip) => chip.textContent)).toEqual([
      "Changed 1 d ago",
    ]);
  });
});
