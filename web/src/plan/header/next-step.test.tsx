import { cleanup, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema, type ChecklistView } from "@/plan/api/schema";
import { NextStepCard } from "@/plan/header/next-step";
import { registerOfferAction } from "@/plan/offers/actions";
import { stoppingAtom } from "@/plan/state/atoms";

afterEach(cleanup);

const checklist = checklistViewSchema.parse(checklistFixture.checklists[0]);
const primary = checklist.primary!;
const secondary = checklist.secondary[0]!;

function renderCard(value: ChecklistView | null, store = createStore()) {
  return render(
    <Provider store={store}>
      <NextStepCard checklist={value} />
    </Provider>,
  );
}

describe("NextStepCard", () => {
  it("describes the primary offer's document in a sentence distinct from its button label", () => {
    renderCard(checklist);

    expect(
      screen.getByTitle("Section 05 is the next product spec document to write.").textContent,
    ).toBe("Section 05 is the next product spec document to write.");
    expect(screen.queryByTitle(primary.label)).toBeNull();
    expect(screen.queryByText("product vision")).toBeNull();
    expect(screen.queryByText(secondary.label)).toBeNull();
  });

  it("names the next plan from the offer's slug instead of repeating its label", () => {
    renderCard({
      ...checklist,
      current: { kind: "step", number: 4 },
      primary: {
        id: "write-plan:feat-planner-01-documents",
        kind: "write_plan",
        label: "Write plan feat-planner-01-documents",
        target: "doc/plans/demo/PLAN-feat-planner-01-documents.md",
        slug: "feat-planner-01-documents",
      },
    });

    expect(
      screen.getByTitle("Plan feat-planner-01-documents is the next plan to write.").textContent,
    ).toBe("Plan feat-planner-01-documents is the next plan to write.");
    expect(screen.queryByTitle("Write plan feat-planner-01-documents")).toBeNull();
  });

  it.each([
    [1, "lucide-telescope"],
    [2, "lucide-book-open"],
    [3, "lucide-map"],
    [4, "lucide-list-checks"],
    [5, "lucide-list-checks"],
    [6, "lucide-file-search"],
    [7, "lucide-list-checks"],
  ] as const)("renders a small DocMark for current step %i", (number, glyphClass) => {
    const { container } = renderCard({
      ...checklist,
      current: { kind: "step", number },
      steps: [{ number, name: "Current step", mark: "gap" }],
    });

    const glyph = container.querySelector(`.${glyphClass}`);
    expect(glyph).not.toBeNull();
    expect(glyph?.classList.contains("size-3")).toBe(true);
    expect(glyph?.parentElement?.classList.contains("size-5")).toBe(true);
    expect(container.querySelectorAll("svg")).toHaveLength(1);
  });

  it("renders exactly one button for the primary, omitting secondary and item offers", () => {
    const unregisterPrimary = registerOfferAction(primary.kind, vi.fn());
    const unregisterSecondary = registerOfferAction(secondary.kind, vi.fn());
    try {
      renderCard(checklist);

      expect(screen.getAllByRole("button")).toHaveLength(1);
      expect(screen.getByRole("button", { name: primary.label })).toBeTruthy();
      expect(screen.queryByText(secondary.label)).toBeNull();
    } finally {
      unregisterPrimary();
      unregisterSecondary();
    }
  });

  it("uses the first checklist item and renders no button without a primary", () => {
    const unregister = registerOfferAction(secondary.kind, vi.fn());
    try {
      renderCard({ ...checklist, primary: null });

      expect(screen.getByTitle("product vision").textContent).toBe("product vision");
      expect(screen.queryByRole("button")).toBeNull();
      expect(screen.queryByText(secondary.label)).toBeNull();
    } finally {
      unregister();
    }
  });

  it.each([
    ["ready", "Ready to run"],
    ["ready_all_done", "All plans done"],
  ] as const)(
    "uses the %s sentence and plan DocMark when no offer or item remains",
    (kind, label) => {
      const { container } = renderCard({
        ...checklist,
        current: { kind },
        primary: null,
        items: [],
      });

      expect(screen.getByTitle(label).textContent).toBe(label);
      expect(container.querySelector(".lucide-list-checks")).not.toBeNull();
      expect(screen.queryByRole("button")).toBeNull();
    },
  );

  it("uses the empty fallback when the current step has no offer or item", () => {
    renderCard({ ...checklist, primary: null, items: [] });

    expect(screen.getByTitle("Nothing to do next").textContent).toBe("Nothing to do next");
  });

  it("wraps the full sentence and action without clamping at any width", () => {
    const { container } = renderCard(checklist);
    const sentence = screen.getByTitle("Section 05 is the next product spec document to write.");
    const action = screen.getByText(primary.label).parentElement;

    expect(sentence.textContent).toBe("Section 05 is the next product spec document to write.");
    expect(sentence.className).not.toContain("line-clamp");
    expect(sentence.classList.contains("break-words")).toBe(true);
    expect(sentence.classList.contains("whitespace-normal")).toBe(true);
    expect(sentence.classList.contains("truncate")).toBe(false);
    expect(sentence.classList.contains("hidden")).toBe(false);
    expect(container.firstElementChild?.classList.contains("min-w-0")).toBe(true);
    expect(container.firstElementChild?.classList.contains("grid")).toBe(true);
    expect(action?.classList.contains("col-span-2")).toBe(true);
    expect(action?.classList.contains("sm:col-start-2")).toBe(true);
    expect(action?.classList.contains("[&_button]:whitespace-normal")).toBe(true);
    expect(action?.classList.contains("[&_button]:h-auto")).toBe(true);
    expect(action?.className).not.toContain("truncate");
  });

  it("renders nothing before the first checklist", () => {
    const { container } = renderCard(null);

    expect(container.innerHTML).toBe("");
  });

  it("keeps the blocked button label and reason readable without truncation", () => {
    const unregister = registerOfferAction(primary.kind, vi.fn());
    const store = createStore();
    store.set(stoppingAtom, true);
    try {
      renderCard(checklist, store);

      const button = screen.getByRole("button", {
        name: primary.label,
        description: "loom plan is stopping",
      });
      const reason = screen.getByText("loom plan is stopping");
      const action = button.parentElement!.parentElement!;
      expect(button.getAttribute("aria-disabled")).toBe("true");
      expect(reason.textContent).toBe("loom plan is stopping");
      expect(action.classList.contains("[&_span]:break-words")).toBe(true);
      expect(action.className).not.toContain("truncate");
    } finally {
      unregister();
    }
  });
});
