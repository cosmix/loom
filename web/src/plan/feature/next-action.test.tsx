import { cleanup, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema, type ChecklistView, type OfferKind } from "@/plan/api/schema";
import { NextAction } from "@/plan/feature/next-action";
import { registerOfferAction } from "@/plan/offers/actions";

const base = checklistViewSchema.parse(checklistFixture.checklists[0]);
const primary = base.primary!;
const secondary = base.secondary[0]!;

const unregister: (() => void)[] = [];

afterEach(() => {
  cleanup();
  unregister.splice(0).forEach((remove) => remove());
});

function register(...kinds: OfferKind[]) {
  for (const kind of kinds) unregister.push(registerOfferAction(kind, vi.fn()));
}

function renderAction(value: ChecklistView, path = "/f/demo/engineering") {
  return render(
    <Provider store={createStore()}>
      <MemoryRouter initialEntries={[path]}>
        <NextAction checklist={value} />
      </MemoryRouter>
    </Provider>,
  );
}

const sentence = () => document.querySelector(".plan-spine-next__text")?.textContent;

describe("the next-step sentence", () => {
  it("names the next section and leaves the label to the button", () => {
    renderAction(base);

    expect(sentence()).toBe("Section 05 is the next product spec document to write.");
    expect(sentence()).not.toContain(primary.label);
  });

  it("names the next plan from the offer's slug", () => {
    renderAction({
      ...base,
      primary: {
        id: "write-plan:demo-01",
        kind: "write_plan",
        label: "Write plan",
        target: "doc/plans/demo/PLAN-demo-01.md",
        slug: "demo-01",
      },
    });

    expect(sentence()).toBe("Plan demo-01 is the next plan to write.");
  });

  it.each([
    [3, "3 plans are untested. You can also start a run."],
    [1, "1 plan is untested. You can also start a run."],
  ] as const)("counts the untested plans in the pressure-test sentence (%i)", (untested, text) => {
    renderAction({
      ...base,
      untested,
      primary: { id: "pt", kind: "pressure_test", label: "Pressure test", slug: "x", target: null },
    });

    expect(sentence()).toBe(text);
  });

  it("names the feature by its slug in a feature-wide offer", () => {
    renderAction({
      ...base,
      feature: "demo-slug",
      primary: { id: "run", kind: "start_run", label: "Start", target: null },
    });

    expect(sentence()).toBe("The plans for demo-slug are ready to run.");
  });

  it("spells out a control character in a server name", () => {
    renderAction({
      ...base,
      primary: { id: "w", kind: "write_plan", label: "Write plan", slug: "de‮mo", target: null },
    });

    expect(sentence()).toBe("Plan de⟨U+202E⟩mo is the next plan to write.");
  });

  it.each([
    ["ready", "Ready to run"],
    ["ready_all_done", "All plans done"],
  ] as const)("falls back to the %s sentence when no offer or item remains", (kind, text) => {
    renderAction({ ...base, current: { kind }, primary: null, items: [] });

    expect(sentence()).toBe(text);
  });

  it("falls back to the first checklist item with no primary", () => {
    renderAction({ ...base, primary: null });

    expect(sentence()).toBe(base.items[0]!.text);
  });
});

describe("the next-step button", () => {
  it("is the one primary button, with no secondary or item offer beside it", () => {
    register(primary.kind, secondary.kind);
    renderAction(base);

    const buttons = screen.getAllByRole("button");
    expect(buttons).toHaveLength(1);
    expect(buttons[0]!.textContent).toBe(primary.label);
    expect(buttons[0]!.getAttribute("data-look")).toBe("primary");
    expect(screen.queryByText(secondary.label)).toBeNull();
  });

  it("is absent without a primary offer", () => {
    register(secondary.kind);
    renderAction({ ...base, primary: null });

    expect(screen.queryByRole("button")).toBeNull();
  });

  it("is left to the pace choice on an overview that shows one", () => {
    const missing = { ...primary, id: "write-section:05" };
    const team = {
      id: "team",
      kind: "draft_all_sections" as const,
      label: "Draft all",
      target: null,
    };
    register(primary.kind);
    renderAction(
      {
        ...base,
        primary: missing,
        secondary: [team],
        items: [{ step: 2, text: "section 05", path: null, offer: missing }],
      },
      "/f/demo/product",
    );

    expect(screen.queryByRole("button")).toBeNull();
    expect(sentence()).toBe("Section 05 is the next product spec document to write.");
  });

  it("keeps its button when the pace choice draws other offers than the primary", () => {
    const guided = { ...primary, id: "write-section:05" };
    const team = {
      id: "team",
      kind: "draft_all_sections" as const,
      label: "Draft all",
      target: null,
    };
    const start = {
      id: "start-vision",
      kind: "start_vision" as const,
      label: "Write vision",
      target: null,
    };
    register("start_vision");
    renderAction(
      {
        ...base,
        primary: start,
        secondary: [team],
        items: [{ step: 2, text: "section 05", path: null, offer: guided }],
      },
      "/f/demo/product",
    );

    expect(screen.getByRole("button", { name: "Write vision" })).toBeTruthy();
  });

  it("returns once the pace choice has nothing left to show", () => {
    register(primary.kind);
    renderAction({ ...base, items: [] }, "/f/demo/product");

    expect(screen.getAllByRole("button")).toHaveLength(1);
  });

  it("is left to a missing vision's page, which draws that offer", () => {
    const start = {
      id: "start-vision",
      kind: "start_vision" as const,
      label: "Write vision",
      target: null,
    };
    register("start_vision");
    const missingVision = { ...base, has_product: false, primary: start, items: [] };

    renderAction(missingVision, "/f/demo/vision");
    expect(screen.queryByRole("button")).toBeNull();
    cleanup();

    renderAction(missingVision, "/f/demo/product/section_01");
    expect(screen.getAllByRole("button")).toHaveLength(1);
  });
});

describe("on a plan's page", () => {
  const planRow = (slug: string, state: string) => ({
    slug,
    path: `doc/plans/demo/PLAN-${slug}.md`,
    title: slug,
    state,
    pressure_date: null,
    listed: true,
  });
  const pressure = (slug: string) => ({
    id: `pressure-test:${slug}`,
    kind: "pressure_test" as const,
    label: `Pressure-test ${slug}`,
    target: `doc/plans/demo/PLAN-${slug}.md`,
    feature: "demo",
    slug,
  });
  const startRun = {
    id: "start-run",
    kind: "start_run" as const,
    label: "Start a run",
    target: null,
  };
  const plans = [planRow("demo-01", "written"), planRow("demo-02", "done")];
  const show = (offer: typeof startRun | ReturnType<typeof pressure>, doc: string) =>
    renderAction(
      { ...base, plan_rows: plans, primary: offer, items: [] },
      `/f/demo/engineering/${doc}`,
    );

  it("is left to the written plan's own Pressure-test button, for that plan only", () => {
    register("pressure_test");
    show(pressure("demo-01"), "demo-01");
    expect(screen.queryByRole("button")).toBeNull();
    cleanup();

    show(pressure("demo-01"), "demo-02");
    expect(screen.getAllByRole("button")).toHaveLength(1);
  });

  it("is left to a written plan's Start a run, and drawn on a plan that has none", () => {
    register("start_run");
    show(startRun, "demo-01");
    expect(screen.queryByRole("button")).toBeNull();
    cleanup();

    show(startRun, "demo-02");
    expect(screen.getAllByRole("button")).toHaveLength(1);
  });
});
