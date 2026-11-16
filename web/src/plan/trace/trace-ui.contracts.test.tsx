import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { expect, it, vi } from "vitest";

import snapshot from "@/plan/api/fixtures/chat-snapshot.json";
import { chatSnapshotSchema, type Turn } from "@/plan/api/schema-chat";
import { TurnFooter } from "@/plan/chat/turn-footer";

import {
  FIRST_PLAN,
  HEADING_NAME,
  S01,
  S03,
  UNCOVERED,
  checklist,
  deepestHolding,
  follows,
  found,
  holding,
  hrefIs,
  norm,
  pageLink,
  patchDoc,
  pressed,
  renderRoutes,
  renderWithProject,
  rowButtons,
  spineNav,
  spineRow,
  tile,
  trace,
  useContractFixtures,
} from "./trace-ui.contracts.helpers";

// The panel's desktop region, without its socket and transcript.
vi.mock("@/plan/chat/panel", async (original) => ({
  ...(await original<typeof import("@/plan/chat/panel")>()),
  ChatPanel: () => <aside aria-label="Chat" />,
  ChatRegistrations: () => null,
}));
// The footer's restore controls need a held session; these contracts read the rows beside them.
vi.mock("@/plan/chat/restore", () => ({ RestoreButton: () => null, RestoreAll: () => null }));
// Offers are observed, never run: no session starts in these contracts.
vi.mock("@/plan/offers/actions", async (original) => ({
  ...(await original<typeof import("@/plan/offers/actions")>()),
  runOffer: vi.fn(() => true),
}));

useContractFixtures();

it("a changed document's spine row names its state", async () => {
  renderRoutes("/f/demo/product");
  // Section 02 (Beta) reads `revised` in the trace fixture.
  const links = await within(await spineNav()).findAllByRole("link", {
    name: (name) => /\bBeta\b/.test(name) && /changed since you settled it/i.test(name),
  });
  // The spine head says the same of Beta; the row is the other link.
  const [row] = links.filter((link) => !link.classList.contains("plan-spine-next"));
  expect(links).toHaveLength(2);
  expect(row!.querySelectorAll("[data-glyph]")).toHaveLength(1);
});

it("a running plan's spine row shows the live dot", async () => {
  checklist.plan_rows[0]!.state = "running";
  patchDoc(FIRST_PLAN, { progress: "running" });
  renderRoutes("/f/demo/engineering/first");
  const row = await spineRow("/f/demo/engineering/first");
  await waitFor(() => expect(row.querySelector(".activity-glyph__led")).not.toBeNull());
  expect(row.querySelector(".aurora-roundel")).toBeNull();
});

it("plan slugs drop the feature prefix in the spine", async () => {
  checklist.plan_rows[0]!.slug = "demo-trace";
  patchDoc(FIRST_PLAN, { slug: "demo-trace" });
  renderRoutes("/f/demo/engineering/first");
  const row = await spineRow("/f/demo/engineering/demo-trace");
  // What a sighted reader sees: the row's text without its `.sr-only` parts.
  const seen = row.cloneNode(true) as HTMLElement;
  seen.querySelectorAll(".sr-only").forEach((node) => node.remove());
  expect(norm(seen.textContent)).toMatch(/\btrace\b/);
  expect(norm(seen.textContent)).not.toContain("demo-trace");
  const titles = [row, ...row.querySelectorAll("[title]")].map((el) => el.getAttribute("title"));
  expect(titles).toContain("demo-trace");
});

it("the spine ends Product spec with Coverage and Plans with Findings, reading the trace", async () => {
  // The trace fixture: four headings, all covered; finding F-1 open and verified, F-2 dismissed.
  renderRoutes("/f/demo/product");
  const spine = within(await spineNav());
  await spine.findByRole("link", { name: /\bAlpha\b/ });
  const expand = spine.queryByRole("button", { name: "Expand Plans" });
  if (expand) fireEvent.click(expand);
  await spine.findAllByRole("link", { name: /\bfirst\b/ });

  const coverage = await spine.findByRole("link", {
    name: "Coverage 4 of 4 headings covered",
    description: "every heading has a plan",
  });
  expect(coverage.getAttribute("href")).toBe("/f/demo/coverage");
  const coverageWell = coverage.closest("li")!;
  expect(coverageWell.getAttribute("data-instrument")).toBe("coverage");
  expect(coverageWell.parentElement?.lastElementChild).toBe(coverageWell);
  expect(coverageWell.querySelector("[data-tone]")).toBeNull();
  // One dot per heading, grouped by the two written sections.
  expect(coverageWell.querySelectorAll(".plan-spine__instrument-section")).toHaveLength(2);
  expect(coverageWell.querySelectorAll("[data-dot='covered']")).toHaveLength(4);

  const findings = await spine.findByRole("link", {
    name: "Findings 1 open",
    description: "from the first review and your reviews · 1 decided",
  });
  expect(findings.getAttribute("href")).toBe("/f/demo/findings");
  const findingsWell = findings.closest("li")!;
  expect(findingsWell.parentElement?.lastElementChild).toBe(findingsWell);
  expect(
    findingsWell.querySelector(".plan-spine__instrument-mark")?.getAttribute("data-tone"),
  ).toBe("warning");
});

it("the Findings row says nothing loud once every finding is decided", async () => {
  for (const { finding } of trace.findings) {
    finding.status = { kind: "dismissed", detail: "settled elsewhere" };
  }
  renderRoutes("/f/demo/engineering/first");
  const spine = within(await spineNav());
  await spine.findAllByRole("link", { name: /\bfirst\b/ });
  const findings = await spine.findByRole("link", { name: "Findings all clear" });
  expect(findings.closest("li")?.querySelector("[data-tone]")).toBeNull();
  expect(findings.closest("li")?.querySelectorAll("[data-dot='decided']")).toHaveLength(2);
});

it("the spine head names the top attention item", async () => {
  // The fixture frontier: section 02 (Beta) revised, then plan first's revisit, then triage.
  expect(trace.frontier.length).toBeGreaterThan(2);
  renderRoutes("/f/demo/product");
  const nav = await spineNav();
  const name = "Review Beta Changed since you settled it";
  const review = await within(nav).findAllByRole("link", { name });
  expect(review).toHaveLength(1);
  expect(review[0]!.getAttribute("href")).toBe("/f/demo/product/section_02");
  // The whole head is the one link: no primary button, and the reason sits inside it.
  const head = nav.querySelector<HTMLElement>(".plan-spine-next")!;
  expect(head).toBe(review[0]);
  expect(head.hasAttribute("data-look")).toBe(false);
  expect(head.classList.contains("plan-button")).toBe(false);
  expect(head.querySelector(".plan-spine-next__reason")?.textContent).toBe(
    "Changed since you settled it",
  );
  expect(nav.textContent).not.toMatch(/Revisit|findings from the last run|Triage|metaplan\b/);
  // The head links the top item alone; the Findings row below it is the spine's own.
  expect(within(head).queryAllByRole("link").filter(hrefIs("/f/demo/findings"))).toEqual([]);
});
it("the turn footer lists documents now out of step", async () => {
  // Plan first covers section 01's headings; the turn's edit put its settled spec out of step.
  patchDoc(FIRST_PLAN, { settle: "upstream_changed" });
  const changed_files = [{ path: S01, before_hash: "before", by_tool: true }];
  const fixture = chatSnapshotSchema.parse(snapshot).turns[0]!;
  const turn: Turn = { ...fixture, id: "t2", open: false, changed_files, summaries_rebuilding: [] };
  const footer = <TurnFooter turn={turn} sessionId="session one" />;
  renderWithProject(<MemoryRouter>{footer}</MemoryRouter>);
  const head = await holding("Now out of step with what you settled");
  const links = () => screen.queryAllByRole("link", { name: /\bFirst\b/ });
  await found(() => links().find((link) => follows(head, link)), "plan First below the heading");
});

it("coverage tiles filter the heading rows", async () => {
  // Section 03 gains headings Five and Six, which nothing covers.
  for (const title of ["Five", "Six"]) {
    const anchor = title.toLowerCase();
    trace.headings.push({ path: S03, anchor, title, covered_by: [], findings: [] });
    trace.gaps.push({ kind: "uncovered", node: { path: S03, anchor }, count: null });
  }
  renderRoutes("/f/demo/coverage");
  await waitFor(() => expect(rowButtons(HEADING_NAME).length).toBeGreaterThanOrEqual(6));
  expect(pressed()).toEqual(["true", "false"]);
  fireEvent.click(tile(/Not covered/));
  await waitFor(() => {
    expect(rowButtons(HEADING_NAME)).toHaveLength(2);
    expect(rowButtons(UNCOVERED)).toHaveLength(2);
  });
  expect(pressed()).toEqual(["false", "true"]);
});

it("a coverage row expands to why, how and findings", async () => {
  renderRoutes("/f/demo/coverage");
  const shownWhy = () => screen.queryAllByText(/^Why$/).filter((el) => !el.closest("[hidden]"));
  const opened = (el: Element) =>
    /\bWhy\b/.test(el.textContent!) && /\bHow\b/.test(el.textContent!);
  // Heading One of section 01 is covered by plan first.
  const row = await found(() => rowButtons(/\bOne\b/)[0], "the row of heading One");
  expect(row.getAttribute("aria-expanded")).toBe("false");
  expect(shownWhy()).toEqual([]);
  fireEvent.click(row);
  await waitFor(() => expect(row.getAttribute("aria-expanded")).toBe("true"));
  const controls = row.getAttribute("aria-controls");
  let open: HTMLElement | null = controls ? document.getElementById(controls) : row;
  while (open && !opened(open)) open = open.parentElement;
  if (!open) throw new Error("the row opened no Why and How");
  expect(open.closest("[hidden]")).toBeNull();
  expect(open.textContent).toMatch(/\bFindings\b/);
  expect(open.textContent).toMatch(/\bfirst\b/);
});

it("findings group by run and fold decided ones", async () => {
  // F-1 is a run finding and F-2 a dismissed one; F-9 comes from Review the feature.
  const [run] = trace.findings;
  const source = "review 2026-11-06";
  trace.findings.push({ ...run!, finding: { ...run!.finding, id: "F-9", source } });
  renderRoutes("/f/demo/findings");
  const [runHead] = await screen.findAllByRole("heading", { name: /^Run of/ });
  const reviews = await screen.findByRole("heading", { name: "Reviews" });
  expect(follows(runHead!, reviews)).toBe(true);
  const line = await holding("Dismissed:", "not needed");
  expect(norm(line.textContent)).toMatch(/Dismissed:\s*not needed/);
  const [folded] = deepestHolding(document.body, "Rename the flag", "Dismissed:");
  expect(within(folded!).queryAllByRole("button", { name: /^Dismiss\b/ })).toEqual([]);
});

it("the product overview draws no Coverage link of its own; the Run page links Findings", async () => {
  renderRoutes("/f/demo/product");
  const nav = await spineNav();
  await screen.findAllByRole("link", { name: /^Coverage\b/ });
  // The spine's row is the one way to Coverage from the overview.
  const outside = screen
    .queryAllByRole("link")
    .filter(hrefIs("/f/demo/coverage"))
    .filter((link) => !nav.contains(link));
  expect(outside).toEqual([]);
  cleanup();
  // Finding F-1 is open and verified in the trace fixture.
  renderRoutes("/f/demo/run");
  await pageLink((name) => /Findings/.test(name) && /\b\d+ open\b/.test(name), "/f/demo/findings");
});
