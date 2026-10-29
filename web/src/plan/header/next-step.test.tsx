import { cleanup, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema, type ChecklistView } from "@/plan/api/schema";
import { NextStepCard } from "@/plan/header/next-step";
import { registerOfferAction } from "@/plan/offers/actions";

afterEach(cleanup);

const checklist = checklistViewSchema.parse(checklistFixture.checklists[0]);
const primary = checklist.primary!;
const secondary = checklist.secondary[0]!;

function renderCard(value: ChecklistView | null) {
  return render(
    <Provider store={createStore()}>
      <NextStepCard checklist={value} />
    </Provider>,
  );
}

describe("NextStepCard", () => {
  it("uses the primary offer's text as the sentence with its full title", () => {
    renderCard(checklist);

    expect(screen.getByTitle("Write section 05").textContent).toBe("Write section 05");
    expect(screen.queryByText("product vision")).toBeNull();
    expect(screen.queryByText(secondary.label)).toBeNull();
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

  it("keeps the sentence truncated and hidden below sm inside a shrinking card", () => {
    const { container } = renderCard(checklist);
    const sentence = screen.getByTitle(primary.label);

    expect(sentence.classList.contains("truncate")).toBe(true);
    expect(sentence.classList.contains("hidden")).toBe(true);
    expect(sentence.classList.contains("sm:block")).toBe(true);
    expect(container.firstElementChild?.classList.contains("min-w-0")).toBe(true);
    expect(container.firstElementChild?.classList.contains("flex-nowrap")).toBe(true);
  });

  it("renders nothing before the first checklist", () => {
    const { container } = renderCard(null);

    expect(container.innerHTML).toBe("");
  });
});
