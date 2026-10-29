import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, expect, it } from "vitest";

import { checklistViewSchema, type ChecklistView } from "@/plan/api/schema";
import { StepList } from "@/plan/feature/step-list";

afterEach(cleanup);

const checklist = checklistViewSchema.parse({
  feature: "demo",
  current: { kind: "step", number: 2 },
  steps: [
    { number: 1, name: "Vision", mark: "done" },
    { number: 2, name: "Product spec", mark: "gap" },
    { number: 3, name: "Metaplan", mark: "active" },
    { number: 4, name: "Plans", mark: "optional" },
    { number: 7, name: "Server-defined step", mark: "optional" },
  ],
  primary: null,
  secondary: [],
  items: [],
});

function renderList(value: ChecklistView) {
  const store = createStore();
  return render(
    <Provider store={store}>
      <StepList checklist={value} />
    </Provider>,
  );
}

it("renders Conversation followed by every server step in an ordered list", () => {
  renderList(checklist);

  const list = screen.getByRole("list", { name: "Steps" });
  expect(list.tagName).toBe("OL");
  expect(within(list).getAllByRole("listitem")).toHaveLength(6);
  expect(
    within(list)
      .getAllByRole("heading")
      .map((heading) => heading.textContent),
  ).toEqual(["Conversation", "Vision", "Product spec", "Metaplan", "Plans", "Server-defined step"]);
});

it.each([
  ["Conversation", "done", "lucide-circle-check"],
  ["Vision", "done", "lucide-circle-check"],
  ["Product spec", "has gaps", "lucide-triangle-alert"],
  ["Metaplan", "in progress", "lucide-circle-dot"],
  ["Plans", "optional", "lucide-circle-dashed"],
] as const)("conveys the %s mark by shape and visible text", (name, label, glyphClass) => {
  renderList(checklist);
  const row = screen.getByRole("heading", { name }).closest("li");
  if (!row) throw new Error("Step heading needs a list row");

  const mark = within(row).getByText(label);
  expect(mark.closest(".sr-only, .hidden, [hidden], [aria-hidden='true']")).toBeNull();
  expect(row.querySelector(`.${glyphClass}`)?.getAttribute("aria-hidden")).toBe("true");
});

it("marks only the current server step with aria-current", () => {
  renderList(checklist);

  const current = screen.getAllByRole("listitem", { current: "step" });
  expect(current).toHaveLength(1);
  const row = current[0];
  if (!row) throw new Error("Checklist needs a current step");
  expect(within(row).getByRole("heading", { name: "Product spec" })).toBeTruthy();
  expect(
    screen
      .getByRole("heading", { name: "Conversation" })
      .closest("li")
      ?.getAttribute("aria-current"),
  ).toBeNull();
});

it.each([
  ["ready", "Ready to run"],
  ["ready_all_done", "All plans done"],
] as const)("shows the %s label without a current step", (kind, label) => {
  renderList(checklistViewSchema.parse({ ...checklist, current: { kind } }));

  expect(screen.queryByRole("listitem", { current: "step" })).toBeNull();
  expect(screen.getByText(label)).toBeTruthy();
});

it("renders server step names through Untrusted", () => {
  renderList(
    checklistViewSchema.parse({
      ...checklist,
      steps: [{ number: 2, name: "Spec\u202Ename", mark: "gap" }],
    }),
  );

  expect(screen.getByRole("heading", { name: "Spec⟨U+202E⟩name" })).toBeTruthy();
});

it("places the pace choice under its supplied step without an active checklist", () => {
  const guided = {
    id: "section:1",
    kind: "write_section",
    label: "Write section 1",
    target: null,
  };
  renderList(
    checklistViewSchema.parse({
      ...checklist,
      primary: guided,
      secondary: [{ id: "team", kind: "draft_all_sections", label: "Draft all", target: null }],
      items: [{ step: 2, text: "Missing section", path: null, offer: guided }],
    }),
  );

  const pace = screen.getByRole("heading", { name: "Guided" });
  const row = pace.closest("li");
  if (!row) throw new Error("Pace choice needs a step row");
  expect(within(row).getByRole("heading", { name: "Product spec" })).toBeTruthy();
  expect(screen.getAllByRole("heading", { name: "Guided" })).toHaveLength(1);
});
