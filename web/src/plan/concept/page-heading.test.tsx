import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

import type { ConceptPage, OutOfDate } from "@/plan/api/schema-concept";

import { show } from "./page-heading-kit";
import { outOfDateText, planStateText, summaryStatus } from "./summary-text";

const openAll = vi.hoisted(() => vi.fn());
vi.mock("./open-all", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./open-all")>()),
  useOpenAll: () => openAll,
}));
vi.mock("./page-header-extras", () => ({
  PageHeaderExtras: ({ page }: { page: ConceptPage }) => (
    <div role="group" aria-label="Header extras">
      {page.key}
    </div>
  ),
  SummaryAction: () => <span>Summary action</span>,
  SummaryDetail: () => <span>Summary detail</span>,
}));

afterEach(() => {
  cleanup();
  openAll.mockClear();
});

it.each([
  ["current", "Summary current", "circle-check", "ok"],
  ["incomplete", "Summary incomplete", "circle-dashed", "neutral"],
  ["stale", "Out of date", "circle-dashed", "warning"],
  ["rebuilding", "Summary rebuilding", "refresh-cw", "accent"],
  ["missing", "No summary yet", "circle-dashed", "neutral"],
] as const)("renders the %s summary status chip", (state, label, glyph, tone) => {
  show({ state: { state, out_of_date: null } });

  const chip = screen.getByText(label).closest("[data-chip]");
  // A rebuilding summary is an operation that ends: the arrow spinner, not an icon.
  const mark = glyph === "refresh-cw" ? "svg.plan-arrow-spinner" : `svg.lucide-${glyph}`;
  expect(chip?.querySelector(mark)).toBeTruthy();
  expect(chip?.getAttribute("data-tone")).toBe(tone);
});

it.each([
  ["source_changed", null, "Out of date: the source changed", "circle-dashed"],
  ["rebuilding", null, "Out of date: rebuilding", "refresh-cw"],
  ["awaiting_approval", null, "Out of date: waiting for approval", "circle-dashed"],
  ["cannot_rebuild", null, "Out of date", "circle-dashed"],
  ["cannot_rebuild", "missing source", "Out of date", "circle-dashed"],
  ["failed", null, "Out of date: build failed", "circle-dashed"],
] as const)("renders a warning for %s (%s)", (reason, detail, label, glyph) => {
  show({ state: { state: "current", out_of_date: { reason, detail } } });

  const chip = screen.getByText(label).closest("[data-chip]");
  expect(chip?.getAttribute("data-tone")).toBe("warning");
  const mark = glyph === "refresh-cw" ? "svg.plan-arrow-spinner" : `svg.lucide-${glyph}`;
  expect(chip?.querySelector(mark)).toBeTruthy();
});

it("sets the summary action at the end of its status row and its detail under the words", () => {
  show({ state: { state: "current", out_of_date: null } });

  const chip = screen.getByText("Summary current").closest("[data-chip]") as HTMLElement;
  expect(chip.tagName).toBe("SPAN");
  const row = chip.parentElement as HTMLElement;
  expect(row.classList.contains("doc-heading-row")).toBe(true);
  expect(row.hasAttribute("data-action")).toBe(true);
  expect(row.querySelector(".doc-heading-row__detail")?.textContent).toBe("Summary detail");
  expect(row.lastElementChild?.className).toBe("doc-heading-row__action");
  expect(row.lastElementChild?.textContent).toBe("Summary action");
});

it("leads the status card with the summary's state, then the plan's state, and sets Built by on the facts line", () => {
  show({
    key: "demo/engineering/my-plan",
    plan_state: "written",
    state: { state: "current", out_of_date: null },
    implemented_by: [{ slug: "feat-planner-09", title: "Other", state: "done" }],
  });

  const card = screen.getByRole("list", { name: "Status" });
  const chips = [...card.querySelectorAll("[data-chip]")];
  expect(chips.map((chip) => chip.querySelector(".doc-heading-chip__label")?.textContent)).toEqual([
    "Summary current",
    "Written",
  ]);
  // Every row draws its glyph column first, so the words share one left edge.
  for (const chip of chips) {
    expect(chip.firstElementChild?.classList.contains("doc-heading-chip__glyph")).toBe(true);
  }
  const facts = card.nextElementSibling as HTMLElement;
  expect(facts.className).toBe("doc-heading-facts");
  expect(facts.textContent).toBe("Built by 09 (Other, done)");
  expect(within(facts).getByRole("link", { name: "09 (Other, done)" })).toBeTruthy();
  expect(within(card).queryByText("Built by", { exact: false })).toBeNull();
});

it("groups the toolbar: the view switch, then Edit, Discuss, the settle control and Open all", () => {
  const { container } = show();

  const toolbar = container.querySelector(".doc-heading-toolbar") as HTMLElement;
  expect(toolbar.previousElementSibling?.querySelector("h1")).not.toBeNull();
  const groups = [...toolbar.children];
  expect(groups.map((group) => group.className)).toEqual(Array(2).fill("doc-heading-group"));
  expect(groups[0]?.contains(screen.getByRole("group", { name: "View" }))).toBe(true);
  // No trace is loaded, so no settle control renders: Open all closes the group.
  expect(groups[1]?.lastElementChild).toBe(screen.getByRole("button", { name: "Open all" }));
});

it("omits summary status when no state is available", () => {
  const { container } = show();

  expect(container.querySelectorAll("[data-chip]")).toHaveLength(0);
  expect(screen.queryByText("Summary action")).toBeNull();
});

it("warns for a stale state without an out-of-date reason", () => {
  show({ state: { state: "stale", out_of_date: null } });

  expect(screen.getByText("Out of date").closest("[data-chip]")?.getAttribute("data-tone")).toBe(
    "warning",
  );
});

it.each([
  ["written", "Written", "pencil-line", "neutral", null],
  [
    "running",
    "Running",
    "refresh-cw",
    "accent",
    "This plan is running. loom run works from the committed copy, so the app keeps it read-only.",
  ],
  ["done", "Done", "circle-check-big", "ok", "This plan is done. Record changes in a new plan."],
  ["retired", "Retired", "archive", "neutral", "This plan is done. Record changes in a new plan."],
] as const)(
  "renders the %s plan state and its lock notice",
  (plan_state, label, glyph, tone, notice) => {
    show({ plan_state });

    const chip = screen.getByText(label).closest("[data-chip]");
    expect(chip?.getAttribute("data-tone")).toBe(tone);
    // "Running" is a state word, so its mark is the two-arrow glyph, never the dot.
    const mark =
      glyph === "refresh-cw" ? "svg.lucide-refresh-cw.plan-arrow-spinner" : `svg.lucide-${glyph}`;
    expect(chip?.querySelector(mark)).not.toBeNull();
    expect(screen.queryByRole("note")?.textContent ?? null).toBe(notice);
  },
);

it("leaves the pressure record off the chip row: a plan's card under the heading draws it", () => {
  show({ last_pressure: "2026-10-20" });
  expect(screen.queryByText("Pressure-tested 2026-10-20")).toBeNull();
  cleanup();

  show({ key: "demo/engineering/a", plan_state: "written", last_pressure: null });
  expect(screen.queryByText(/pressure-tested/i)).toBeNull();
});

it("ends a written plan's state row in Pressure-test, and only a written plan's", () => {
  show({ key: "demo/engineering/a", path: "plans/demo/PLAN-a.md", plan_state: "written" });
  const row = screen.getByText("Written").closest(".doc-heading-row") as HTMLElement;
  expect(within(row).getByRole("button", { name: "Pressure-test" })).toBeTruthy();
  cleanup();

  show({ key: "demo/engineering/a", path: "plans/demo/PLAN-a.md", plan_state: "running" });
  expect(screen.queryByRole("button", { name: "Pressure-test" })).toBeNull();
});

it("leaves the pressure chip off a section summary with no test date", () => {
  show();

  expect(screen.queryByText(/pressure-tested/i)).toBeNull();
});

it("renders unstated implements metadata as one chip", () => {
  show({ implements: { kind: "not_stated" } });

  expect(screen.getByText("Implements: not stated").closest("[data-chip]")).not.toBeNull();
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
            path: "doc/product/demo/section_01.md",
            page: "demo/product/section_01",
          },
        },
        { number: 99, name: "section_99", doc: null },
      ],
    },
  });

  const chip = screen.getByText("Implements", { exact: false }).closest("[data-chip]");
  expect(chip?.textContent).toBe("Implements: §01, section_99");
  const link = screen.getByRole("link", { name: "§01" });
  expect(link.getAttribute("href")).toBe("/f/demo/product/section_01");
  expect(link.getAttribute("title")).toBe("Planning");
  const unresolved = screen.getByText("section_99");
  expect(unresolved.closest("a")).toBeNull();
  expect(unresolved.closest("[title]")).toBeNull();
});

it("renders no Implements chip for a sections view with no links", () => {
  show({ implements: { kind: "sections", links: [] } });

  expect(screen.queryByText(/Implements/)).toBeNull();
});

it("chips the implementing plans as Built by their numbers, each linked with its state", () => {
  show({
    implemented_by: [
      { slug: "feat-planner.05", title: "Concept mode", state: "in_progress" },
      { slug: "feat-planner-09-reading", title: "Second plan", state: "done" },
    ],
  });

  const chip = screen.getByText("Built by", { exact: false }).closest("[data-chip]");
  expect(chip?.textContent).toBe(
    "Built by 05 (Concept mode, in progress) · 09 (Second plan, done)",
  );
  const link = screen.getByRole("link", { name: "05 (Concept mode, in progress)" });
  expect(link.getAttribute("href")).toBe("/f/demo/engineering/feat-planner.05");
  expect(screen.getByRole("link", { name: "09 (Second plan, done)" }).getAttribute("href")).toBe(
    "/f/demo/engineering/feat-planner-09-reading",
  );
});

it("opens every sentence from the quiet Open all heading button", () => {
  show();

  const button = screen.getByRole("button", { name: "Open all" });
  expect(button.querySelector("svg.lucide-unfold-vertical")).not.toBeNull();
  fireEvent.click(button);
  expect(openAll).toHaveBeenCalledOnce();
});

it("switches from summary to the encoded source address without a diff link", () => {
  show({ path: "doc/plans/PLAN-a b#.md" });

  const views = screen.getByRole("group", { name: "View" });
  expect(within(views).getByText("Summary").getAttribute("aria-current")).toBe("page");
  expect(within(views).getByRole("link", { name: "Source" }).getAttribute("href")).toBe(
    "/files/plans/PLAN-a%20b%23.md",
  );
  expect(screen.queryByText("doc/plans/PLAN-a b#.md")).toBeNull();
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
  ["overview/demo/product", "telescope"],
  ["overview/demo/engineering", "map"],
  ["demo/engineering/my-plan", "list-checks"],
  ["plans/my-plan", "list-checks"],
] as const)("takes the document kind from %s regardless of tint", (key, glyph) => {
  for (const colour of [1, 3]) {
    const { container, unmount } = show({ key, colour });

    expect(container.querySelector(`svg.lucide-${glyph}`)).not.toBeNull();
    unmount();
  }
});

it.each([
  ["demo/product/section_03", "03"],
  ["demo/engineering/feat-planner-13", "13"],
])("leads the title of %s with its muted number and draws no document mark", (key, number) => {
  const { container } = show({ key, title: "Planning" });

  const heading = screen.getByRole("heading", { level: 1, name: "Planning" });
  expect(heading.querySelector(".doc-heading-number")?.textContent).toBe(number);
  expect(container.querySelector("svg.lucide-book-open, svg.lucide-list-checks")).toBeNull();
});

it("keeps standalone plan breadcrumbs unlinked", () => {
  show({ key: "plans/my-plan", feature: null });

  const nav = screen.getByRole("navigation", { name: "Breadcrumb" });
  expect(within(nav).getByText("Plans").closest("a")).toBeNull();
});

it("links a standalone implementing plan", () => {
  show({
    feature: null,
    implemented_by: [{ slug: "my-plan", title: "My plan", state: "written" }],
  });

  expect(screen.getByRole("link", { name: "My plan (written)" }).getAttribute("href")).toBe(
    "/plans/my-plan",
  );
});

it.each([
  { state: "rebuilding", out_of_date: null },
  { state: "current", out_of_date: { reason: "rebuilding", detail: null } },
] as const)("shows a rebuilding mark for $state / $out_of_date", (state) => {
  const { container } = show({ key: "demo/engineering/my-plan", colour: 5, state });

  expect(container.querySelector('[data-kind="summaries"]')).not.toBeNull();
  expect(container.querySelector("svg.lucide-list-checks")).toBeNull();
});

it("mounts PageHeaderExtras inside the heading with the same page", () => {
  show({ key: "demo/engineering/my-plan" });

  const extras = screen.getByRole("group", { name: "Header extras" });
  expect(extras.textContent).toBe("demo/engineering/my-plan");
  expect(extras.closest("header")).not.toBeNull();
});

it.each([
  ["source_changed", null, "Out of date: the source changed"],
  ["rebuilding", null, "Out of date: rebuilding"],
  ["awaiting_approval", null, "Out of date: waiting for approval"],
  ["cannot_rebuild", null, "Out of date"],
  ["cannot_rebuild", "", "Out of date"],
  ["cannot_rebuild", "missing source", "Out of date"],
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
