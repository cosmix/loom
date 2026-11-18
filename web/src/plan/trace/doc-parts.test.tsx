import { cleanup, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { pressureWhen } from "@/plan/files/heading-parts";

import { ImpactRows } from "./impact-rows";
import {
  FIRST_PLAN,
  METAPLAN,
  S01,
  S02,
  VISION,
  kit,
  mount,
  patchDoc,
  resetKit,
} from "./components-kit";
import { PassageCoverage } from "./passage-coverage";
import { PlanExtras } from "./plan-extras";
import { ScopeLine } from "./scope-line";

beforeEach(resetKit);

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const traceFetched = () =>
  waitFor(() =>
    expect(kit.calls.some((call) => call.url.pathname === "/api/trace/demo")).toBe(true),
  );

// ---- Scope standfirst ------------------------------------------------------------------------

it("a plan's scope is its metaplan entry's first paragraph, linked to the entry", async () => {
  mount(<ScopeLine feature="demo" path={FIRST_PLAN} />);
  const link = await screen.findByRole("link", { name: /from the metaplan/ });
  expect(link.getAttribute("href")).toBe("/f/demo/source/plans/demo/index.md#1-first-plan-firstmd");
  const scope = document.querySelector("p.trace-scope")!;
  await waitFor(() => expect(scope.textContent).toContain("Build the first plan from one idea."));
  expect(scope.textContent).not.toContain("`PLAN-first.md`");
});

it("a section's scope is its vision row's Covers cell, and a changed stub is noted", async () => {
  patchDoc(S02, { settle: "stub_changed" });
  mount(<ScopeLine feature="demo" path={S02} />);
  await screen.findByText("What beta promises");
  const link = screen.getByRole("link", { name: /from the vision/ });
  expect(link.getAttribute("href")).toContain("/f/demo/source/product/demo/index.md");
  const note = screen.getByRole("note");
  expect(note.textContent).toMatch(/^The vision row changed since you settled this/);
  expect(within(note).getByRole("link", { name: "See what changed" })).toBeTruthy();
});

it("a section whose stub is unchanged carries no changed note", async () => {
  mount(<ScopeLine feature="demo" path={S02} />);
  await screen.findByText("What beta promises");
  expect(screen.queryByRole("note")).toBeNull();
});

it("a done plan's changed stub asks nothing: the scope shows, the note does not", async () => {
  patchDoc(FIRST_PLAN, { settle: "stub_changed", settleable: false });
  mount(<ScopeLine feature="demo" path={FIRST_PLAN} />);
  await screen.findByRole("link", { name: /from the metaplan/ });
  expect(screen.queryByRole("note")).toBeNull();
});

it("the vision and the metaplan have no scope", async () => {
  mount(
    <>
      <ScopeLine feature="demo" path={VISION} />
      <ScopeLine feature="demo" path={METAPLAN} />
    </>,
  );
  await traceFetched();
  expect(screen.queryByRole("link")).toBeNull();
});

// ---- Plan page -------------------------------------------------------------------------------

const RECORD = "A team of three read the plan; two points are addressed.";

function withPressureRecord() {
  kit.docs["plans/demo/PLAN-first.md"] = {
    text: `# First\n\n## Pressure tests\n\n- 2026-10-20: ${RECORD}\n`,
    sections: [
      { level: 2, title: "Pressure tests", anchor: "pressure-tests", start_line: 3, end_line: 5 },
    ],
  };
}

it("a plan with a pressure record shows its date, its text and a link to it", async () => {
  withPressureRecord();
  mount(<PlanExtras feature="demo" path={FIRST_PLAN} pressure="2026-10-20" />);
  const card = (await screen.findByRole("heading", { name: "Pressure record" })).closest(
    "section",
  )!;
  expect(card.textContent).toContain(`Pressure-tested ${pressureWhen("2026-10-20")}`);
  await waitFor(() => expect(card.textContent).toContain(RECORD));
  const link = within(card).getByRole("link", { name: "Read the record" });
  expect(link.getAttribute("href")).toBe("/f/demo/source/plans/demo/PLAN-first.md#pressure-tests");
});

it("a plan with no pressure record has no card", async () => {
  mount(<PlanExtras feature="demo" path={FIRST_PLAN} pressure={null} />);
  await screen.findByRole("region", { name: "What it implements" });
  expect(screen.queryByRole("heading", { name: "Pressure record" })).toBeNull();
});

it("what a plan implements lists its headings with section number, title and state", async () => {
  mount(<PlanExtras feature="demo" path={FIRST_PLAN} pressure={null} />);
  const block = await screen.findByRole("region", { name: "What it implements" });
  expect(block.textContent).toContain("2 spec headings in 1 section");
  const rows = within(block).getAllByRole("listitem");
  expect(rows).toHaveLength(2);
  expect(rows[0]!.textContent).toBe("01 One · AlphaSettled");
  expect(within(rows[0]!).getByRole("link").getAttribute("href")).toBe(
    "/f/demo/source/product/demo/section_01.md#one",
  );
  // Section 02's headings belong to plan second's stub, not to plan first.
  expect(block.textContent).not.toMatch(/\b(Three|Four)\b/);
});

it("a plan's briefs are grouped by stage folder, each a mono file name", async () => {
  const files = ["build/r1.md", "build/r2.md", "verify/v1.md"];
  patchDoc(FIRST_PLAN, { bundle: files.map((rel) => `doc/plans/demo/briefs/first/${rel}`) });
  mount(<PlanExtras feature="demo" path={FIRST_PLAN} pressure={null} />);
  const list = await screen.findByRole("list", { name: "Briefs" });
  const folders = within(list).getAllByRole("heading");
  expect(folders.map((folder) => folder.textContent)).toEqual(["build", "verify"]);
  expect(within(list).getByRole("link", { name: "v1.md" }).getAttribute("href")).toBe(
    "/f/demo/source/plans/demo/briefs/first/verify/v1.md",
  );
});

// ---- Opened passage --------------------------------------------------------------------------

it("a covered passage names the plan and stage, and its open findings", async () => {
  mount(<PassageCoverage path={S01} anchor="two" />);
  const line = await screen.findByText(/Covered by/);
  expect(line.textContent).toBe("Covered by first › build · 1 open finding");
  const links = within(line).getAllByRole("link");
  expect(links.map((link) => link.getAttribute("href"))).toEqual([
    "/f/demo/engineering/first",
    "/f/demo/findings",
  ]);
});

it("a heading covered as a whole by a plan shows the plan alone", async () => {
  mount(<PassageCoverage path={S01} anchor="one" />);
  const line = await screen.findByText(/Covered by/);
  expect(line.textContent).toBe("Covered by first");
});

it("an uncovered passage says so and links to Coverage at its heading", async () => {
  kit.trace.headings.find((h) => h.path === S01 && h.anchor === "two")!.covered_by = [];
  mount(<PassageCoverage path={S01} anchor="two" />);
  const link = await screen.findByRole("link", { name: "Not covered by any plan" });
  expect(link.getAttribute("href")).toBe("/f/demo/coverage#section_01%3Atwo");
});

it("a passage the trace has no heading for draws no coverage line", async () => {
  mount(
    <>
      <PassageCoverage path={S01} anchor="nowhere" />
      <PassageCoverage path="doc/loom/knowledge/notes.md" anchor="one" />
    </>,
  );
  await traceFetched();
  expect(document.querySelector(".concept-passage-coverage")).toBeNull();
});

// ---- Turn footer -----------------------------------------------------------------------------

const heading = "Now out of step with what you settled";

it("impact rows list the plan covering a changed section with its glyph and state", async () => {
  patchDoc(FIRST_PLAN, { settle: "upstream_changed" });
  mount(<ImpactRows path={S01} />);
  const list = await screen.findByRole("list", { name: "Affected documents" });
  expect(screen.getByText(heading)).toBeTruthy();
  expect(list.textContent).toBe("First Spec changed since you settled it");
  expect(list.querySelector("[data-glyph='attention']")).not.toBeNull();
});

it("impact rows list a done plan covering a changed section without a settle state", async () => {
  patchDoc(FIRST_PLAN, { settle: "upstream_changed", settleable: false });
  mount(<ImpactRows path={S01} />);
  const list = await screen.findByRole("list", { name: "Affected documents" });
  expect(list.textContent).toBe("First");
  expect(list.querySelector("[data-settle]")).toBeNull();
});

it("impact rows leave out a related document that moved but is done work", async () => {
  patchDoc(S01, { settle: "revised", settleable: false });
  mount(<ImpactRows path={FIRST_PLAN} />);
  await traceFetched();
  expect(screen.queryByRole("list", { name: "Affected documents" })).toBeNull();
  expect(screen.queryByText(heading)).toBeNull();
});

it("impact rows list a section that moved when its plan changed", async () => {
  patchDoc(S01, { settle: "revised" });
  mount(<ImpactRows path={FIRST_PLAN} />);
  const list = await screen.findByRole("list", { name: "Affected documents" });
  expect(list.textContent).toMatch(/Changed since you settled it/);
});

it("impact rows draw nothing for a document outside every feature", async () => {
  mount(<ImpactRows path="doc/loom/knowledge/notes.md" />);
  expect(screen.queryByRole("list")).toBeNull();
  expect(screen.queryByText(heading)).toBeNull();
});
