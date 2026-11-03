import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import { featureOfKey } from "@/plan/api/page-key";
import { checklistViewSchema } from "@/plan/api/schema";
import { conceptPageSchema, type ConceptPage, type OutOfDate } from "@/plan/api/schema-concept";
import { projectAtom } from "@/plan/state/atoms";

import { PageHeading } from "./page-heading";
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
}));

afterEach(() => {
  cleanup();
  openAll.mockClear();
});

function pageWith(overrides: Partial<ConceptPage> = {}): ConceptPage {
  return conceptPageSchema.parse({
    ...structuredClone(summaryFixture),
    state: null,
    implemented_by: [],
    feature: featureOfKey(overrides.key ?? summaryFixture.key),
    ...overrides,
  });
}

function show(overrides: Partial<ConceptPage> = {}) {
  const page = pageWith(overrides);
  return render(<PageHeading page={page} />, { wrapper: MemoryRouter });
}

it.each([
  ["current", "Summary current", "circle-check", "ok"],
  ["incomplete", "Summary incomplete", "circle-dashed", "neutral"],
  ["stale", "Out of date", "circle-dashed", "warning"],
  ["rebuilding", "Summary rebuilding", "refresh-cw", "accent"],
  ["missing", "No summary yet", "circle-dashed", "neutral"],
] as const)("renders the %s summary status chip", (state, label, glyph, tone) => {
  show({ state: { state, out_of_date: null } });

  const chip = screen.getByText(label).closest("[data-chip]");
  expect(chip?.querySelector(`svg.lucide-${glyph}`)).toBeTruthy();
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
  expect(chip?.querySelector(`svg.lucide-${glyph}`)).toBeTruthy();
});

it("sets the summary action right after the status chip, on its baseline", () => {
  show({ state: { state: "current", out_of_date: null } });

  const chip = screen.getByText("Summary current").closest("[data-chip]") as HTMLElement;
  expect(chip.nextElementSibling?.textContent).toBe("Summary action");
  const pair = chip.parentElement as HTMLElement;
  expect(pair.classList.contains("items-baseline")).toBe(true);
  expect(pair.children).toHaveLength(2);
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
    expect(chip?.querySelector(`svg.lucide-${glyph}`)).not.toBeNull();
    expect(screen.queryByRole("note")?.textContent ?? null).toBe(notice);
  },
);

it("renders the last pressure test as an ok chip and an untested plan as a warning", () => {
  show({ last_pressure: "2026-10-20" });

  const tested = screen.getByText("Pressure-tested 2026-10-20").closest("[data-chip]");
  expect(tested?.getAttribute("data-tone")).toBe("ok");
  expect(tested?.querySelector("svg.lucide-flask-conical")).not.toBeNull();
  cleanup();

  show({ key: "demo/engineering/a", plan_state: "written", last_pressure: null });
  const untested = screen.getByText("Not pressure-tested").closest("[data-chip]");
  expect(untested?.getAttribute("data-tone")).toBe("warning");
  expect(untested?.querySelector("svg.lucide-gauge")).not.toBeNull();
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

it.each([
  ["demo/product/section_01", "Product spec", "/f/demo/product", "Section 01"],
  ["overview/demo/product", "Product spec", "/f/demo/product", "Planning"],
  ["demo/engineering/my-plan", "Plans", "/f/demo/engineering", "my-plan"],
  ["overview/demo/engineering", "Engineering", "/f/demo/engineering", "Planning"],
] as const)("renders the area breadcrumb for %s", (key, label, to, current) => {
  show({ key });

  const nav = screen.getByRole("navigation", { name: "Breadcrumb" });
  expect(within(nav).queryByText("Planner")).toBeNull();
  expect(within(nav).getByRole("link", { name: label }).getAttribute("href")).toBe(to);
  expect(nav.querySelector('[aria-current="page"]')?.textContent).toBe(current);
});

it("starts the breadcrumb at the feature's slug, linking to its screen", () => {
  const store = createStore();
  store.set(projectAtom, {
    checklists: [
      {
        ...checklistViewSchema.parse(structuredClone(checklistFixture.checklists[0])),
        feature: "demo",
        sections: Array.from({ length: 12 }, (_, index) => ({
          number: String(index + 1).padStart(2, "0"),
          name: `S${index + 1}`,
          written: true,
          open_questions: 0,
        })),
      },
    ],
    standalone: [],
    moves: [],
  });
  render(<PageHeading page={pageWith({ key: "demo/product/section_05" })} />, {
    wrapper: ({ children }) => (
      <Provider store={store}>
        <MemoryRouter>{children}</MemoryRouter>
      </Provider>
    ),
  });

  const nav = screen.getByRole("navigation", { name: "Breadcrumb" });
  expect(
    within(nav)
      .getAllByRole("listitem")
      .map((item) => item.textContent),
  ).toEqual(["demo", "Product spec", "Section 05 of 12"]);
  const first = within(nav).getByRole("link", { name: "demo" });
  expect(first.getAttribute("href")).toBe("/f/demo");
  expect(first.querySelector("svg.lucide-layers")).not.toBeNull();
});

it("sets a plan summary's slug crumb in mono", () => {
  show({ key: "demo/engineering/my-plan" });

  const nav = screen.getByRole("navigation", { name: "Breadcrumb" });
  expect(within(nav).getByText("my-plan").closest("[aria-current]")?.className).toContain(
    "font-mono",
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
  ["demo/product/section_03", "book-open"],
  ["demo/engineering/my-plan", "list-checks"],
  ["plans/my-plan", "list-checks"],
] as const)("takes the document kind from %s regardless of tint", (key, glyph) => {
  for (const colour of [1, 3]) {
    const { container, unmount } = show({ key, colour });

    expect(container.querySelector(`svg.lucide-${glyph}`)).not.toBeNull();
    unmount();
  }
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
  const { container } = show({ colour: 5, state });

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
