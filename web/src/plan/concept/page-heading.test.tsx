import { cleanup, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import { conceptPageSchema, type ConceptPage, type OutOfDate } from "@/plan/api/schema-concept";

import { PageHeading } from "./page-heading";
import { outOfDateText, planStateText, summaryStatus } from "./summary-text";

vi.mock("./page-header-extras", () => ({
  PageHeaderExtras: ({ page }: { page: ConceptPage }) => (
    <div role="group" aria-label="Header extras">
      {page.key}
    </div>
  ),
}));

afterEach(cleanup);

function pageWith(overrides: Partial<ConceptPage> = {}): ConceptPage {
  return conceptPageSchema.parse({
    ...structuredClone(summaryFixture),
    state: null,
    implemented_by: [],
    ...overrides,
  });
}

function show(overrides: Partial<ConceptPage> = {}) {
  const page = pageWith(overrides);
  return render(<PageHeading page={page} />, { wrapper: MemoryRouter });
}

it.each([
  ["current", "Up to date", "circle-check"],
  ["incomplete", "Summary incomplete", "circle-dashed"],
  ["stale", "Out of date", "circle-dashed"],
  ["rebuilding", "Rebuilding", "refresh-cw"],
  ["missing", "No summary yet", "circle-dashed"],
] as const)("renders the %s summary status chip", (state, label, glyph) => {
  show({ state: { state, out_of_date: null } });

  const chip = screen.getByText(label).closest(".rounded-full");
  expect(chip?.querySelector(`svg.lucide-${glyph}`)).toBeTruthy();
});

it.each([
  ["source_changed", null, "Out of date: the source changed", "circle-dashed"],
  ["rebuilding", null, "Out of date: rebuilding", "refresh-cw"],
  ["awaiting_approval", null, "Out of date: waiting for approval", "circle-dashed"],
  ["cannot_rebuild", null, "Out of date: cannot rebuild", "circle-dashed"],
  [
    "cannot_rebuild",
    "missing source",
    "Out of date: cannot rebuild: missing source",
    "circle-dashed",
  ],
  ["failed", null, "Out of date: build failed", "circle-dashed"],
] as const)("renders a warning for %s (%s)", (reason, detail, label, glyph) => {
  show({ state: { state: "current", out_of_date: { reason, detail } } });

  const chip = screen.getByText(label).closest(".rounded-full");
  expect(chip?.classList.contains("text-(--tone-warning)")).toBe(true);
  expect(chip?.querySelector(`svg.lucide-${glyph}`)).toBeTruthy();
});

it("omits summary status when no state is available", () => {
  const { container } = show();

  expect(container.querySelectorAll(".rounded-full")).toHaveLength(0);
});

it("warns for a stale state without an out-of-date reason", () => {
  show({ state: { state: "stale", out_of_date: null } });

  expect(
    screen
      .getByText("Out of date")
      .closest(".rounded-full")
      ?.classList.contains("text-(--tone-warning)"),
  ).toBe(true);
});

it.each([
  ["written", "pencil-line", null],
  ["running", "play", "This plan is running. `loom run` works from the committed copy."],
  ["done", "circle-check-big", "This plan is done. Record changes in a new plan."],
  ["retired", "archive", "This plan is done. Record changes in a new plan."],
] as const)("renders the %s plan state and its lock notice", (plan_state, glyph, notice) => {
  show({ plan_state });

  const chip = screen.getByTitle(`Plan state: ${plan_state}`);
  expect(chip.textContent).toBe(plan_state);
  expect(chip.querySelector(`svg.lucide-${glyph}`)).not.toBeNull();
  expect(screen.queryByRole("note")?.textContent ?? null).toBe(notice);
});

it("renders the last pressure test as a separate chip", () => {
  show({ last_pressure: "2026-10-20" });

  expect(screen.getByText("Last pressure test 2026-10-20").closest(".rounded-full")).not.toBeNull();
});

it("renders unstated implements metadata as one chip", () => {
  show({ implements: { kind: "not_stated" } });

  expect(screen.getByText("Implements: not stated").closest(".rounded-full")).not.toBeNull();
});

it("links implemented sections and keeps unresolved section names unlinked", () => {
  show({
    implements: {
      kind: "sections",
      links: [
        {
          number: 1,
          name: "section_01",
          doc: {
            id: "section_01",
            title: "Planning",
            path: "doc/product/section_01.md",
            page: "product/section_01",
          },
        },
        { number: 99, name: "section_99", doc: null },
      ],
    },
  });

  expect(screen.getByText("Implements:")).toBeTruthy();
  expect(screen.getByRole("link", { name: "Planning" }).getAttribute("href")).toBe(
    "/product/section_01",
  );
  const unresolved = screen.getByText("section_99");
  expect(unresolved.closest(".rounded-full")).not.toBeNull();
  expect(unresolved.closest("a")).toBeNull();
});

it("links implementing plans with their human-readable states", () => {
  show({
    implemented_by: [
      { slug: "feat-planner.05", title: "Concept mode", state: "in_progress" },
      { slug: "plan-two", title: "Second plan", state: "done" },
    ],
  });

  expect(screen.getByText("Implemented by:")).toBeTruthy();
  const link = screen.getByRole("link", { name: "Concept mode (in progress)" });
  expect(link.getAttribute("href")).toBe("/engineering/feat-planner.05");
  expect(link.getAttribute("title")).toBe("Concept mode");
  expect(screen.getByRole("link", { name: "Second plan (done)" }).getAttribute("href")).toBe(
    "/engineering/plan-two",
  );
});

it.each([
  ["product/section_01", "Product", "/product"],
  ["product", "Product", "/product"],
  ["engineering/my-plan", "Plans", "/engineering"],
  ["engineering", "Plans", "/engineering"],
] as const)("renders the group breadcrumb for %s", (key, label, to) => {
  show({ key });

  const nav = screen.getByRole("navigation", { name: "Breadcrumb" });
  expect(within(nav).getByText("Planner").closest("a")).toBeNull();
  expect(within(nav).getByRole("link", { name: label }).getAttribute("href")).toBe(to);
  expect(nav.querySelector('[aria-current="page"]')?.textContent).toBe("Planning");
});

it("switches from summary to the encoded source address without a diff link", () => {
  show({ path: "doc/plans/PLAN-a b#.md" });

  const views = screen.getByRole("group", { name: "View" });
  expect(within(views).getByText("Summary").getAttribute("aria-current")).toBe("page");
  expect(within(views).getByRole("link", { name: "Source" }).getAttribute("href")).toBe(
    "/files/plans/PLAN-a%20b%23.md",
  );
  expect(screen.getByText("doc/plans/PLAN-a b#.md")).toBeTruthy();
  expect(screen.queryByRole("link", { name: "Uncommitted changes" })).toBeNull();
});

it("disables the source view when there is no source path", () => {
  show({ path: null });

  const views = screen.getByRole("group", { name: "View" });
  expect(within(views).getByText("Source").getAttribute("aria-disabled")).toBe("true");
  expect(within(views).queryByRole("link")).toBeNull();
  expect(screen.queryByRole("button", { name: "Copy path" })).toBeNull();
});

it.each([
  [1, "telescope"],
  [2, "book-open"],
  [3, "sticky-note"],
  [4, "map"],
  [5, "list-checks"],
  [6, "file-text"],
  [7, "library"],
  [8, "file-search"],
] as const)("maps colour %s to the corresponding document mark", (colour, glyph) => {
  const { container } = show({ colour });

  expect(container.querySelector(`svg.lucide-${glyph}`)).not.toBeNull();
});

it.each([null, 0, 255])("omits the document mark for unmatched colour %s", (colour) => {
  show({ colour });

  const title = screen.getByRole("heading", { level: 1, name: "Planning" });
  expect(title.previousElementSibling).toBeNull();
});

it.each([
  { state: "rebuilding", out_of_date: null },
  { state: "current", out_of_date: { reason: "rebuilding", detail: null } },
] as const)("shows a rebuilding mark for $state / $out_of_date", (state) => {
  const { container } = show({ colour: 5, state });

  expect(container.querySelector('[data-kind="summaries"]')).not.toBeNull();
  expect(container.querySelector("svg.lucide-list-checks")).toBeNull();
});

it("mounts PageHeaderExtras inside the heading with the same page", () => {
  show({ key: "engineering/my-plan" });

  const extras = screen.getByRole("group", { name: "Header extras" });
  expect(extras.textContent).toBe("engineering/my-plan");
  expect(extras.closest("header")).not.toBeNull();
});

it.each([
  ["source_changed", null, "Out of date: the source changed"],
  ["rebuilding", null, "Out of date: rebuilding"],
  ["awaiting_approval", null, "Out of date: waiting for approval"],
  ["cannot_rebuild", null, "Out of date: cannot rebuild"],
  ["cannot_rebuild", "", "Out of date: cannot rebuild"],
  ["cannot_rebuild", "missing source", "Out of date: cannot rebuild: missing source"],
  ["failed", null, "Out of date: build failed"],
] satisfies [OutOfDate["reason"], string | null, string][])(
  "preserves the extracted wording for %s (%s)",
  (reason, detail, expected) => {
    expect(outOfDateText({ reason, detail })).toBe(expected);
  },
);

it("preserves the extracted null status and plan state formatting", () => {
  expect(summaryStatus(null)).toBeNull();
  expect(planStateText("in_progress_now")).toBe("in progress now");
});
