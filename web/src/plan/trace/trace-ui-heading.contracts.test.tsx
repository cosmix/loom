import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";

import {
  FIRST_PLAN,
  S01,
  SCOPE,
  deepestHolding,
  follows,
  found,
  holding,
  hrefIs,
  patchDoc,
  renderRoutes,
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

it("a changed document offers See what changed", async () => {
  // Section 02 reads `revised` in the trace fixture.
  renderRoutes("/f/demo/product/section_02");
  await screen.findAllByRole("button", { name: /changed since you settled it/i });
  const named = (name: string) =>
    ["link", "button"].flatMap((role) => screen.queryAllByRole(role, { name }));
  await waitFor(() => expect(named("See what changed")).not.toEqual([]));
  expect(named("Changes since settled")).toEqual([]);
});

it("a changed document says so once, on the settle button, with See what changed on its row", async () => {
  renderRoutes("/f/demo/product/section_02");
  const settle = await screen.findByRole("button", { name: "Changed since you settled it" });
  const header = settle.closest("header") as HTMLElement;
  // The state is the button's label: no chip or line of the heading repeats it.
  expect(within(header).getAllByText(/changed since you settled it/i)).toHaveLength(1);
  const row = settle.closest(".doc-heading-state") as HTMLElement;
  const changes = await within(row).findByRole("button", { name: "See what changed" });
  expect(changes.className).toBe(
    settle.className.replace(" trace-settle-button", " trace-changes"),
  );
});

it("the scope shows as a standfirst linking its stub", async () => {
  renderRoutes("/f/demo/product/section_03");
  const settle = await screen.findByRole("button", { name: "Mark settled" });
  const headers = () => [...document.querySelectorAll("header")];
  const header = await found(() => headers().find((el) => el.querySelector("h1")), "the heading");
  // Section 03's stub is its row in the vision's scope table.
  const scope = await found(() => deepestHolding(header, SCOPE)[0], "the vision row");
  expect(scope.closest("[hidden], details:not([open])")).toBeNull();
  expect(follows(scope, settle)).toBe(true);
  const from = within(header).getByRole("link", { name: /from the vision/i });
  expect(from.getAttribute("href")).toMatch(
    /^\/f\/demo\/(vision|source\/product\/demo\/index\.md)/,
  );
});

it("a plan page lists what it implements and its briefs", async () => {
  const files = ["build/r1.md", "build/r2.md", "verify/v1.md"];
  patchDoc(FIRST_PLAN, { bundle: files.map((rel) => `doc/plans/demo/briefs/first/${rel}`) });
  renderRoutes("/f/demo/engineering/first");
  const name = "What it implements";
  const blocks = () => ["region", "list"].flatMap((role) => screen.queryAllByRole(role, { name }));
  const block = await found(() => blocks()[0], name);
  // Plan first covers heading One of section 01; Three and Four are plan second's.
  await waitFor(() => expect(block.textContent).toMatch(/\bOne\b/));
  expect(block.textContent).not.toMatch(/\b(Three|Four)\b/);
  const briefs = await screen.findByRole("list", { name: "Briefs" });
  for (const file of files) expect(briefs.textContent).toContain(file.split("/").pop());
});

it("an opened passage says what covers it", async () => {
  // Heading two loses its cover, so sentence s2 rests on an uncovered heading.
  trace.headings.find((h) => h.path === S01 && h.anchor === "two")!.covered_by = [];
  renderRoutes("/f/demo/product/section_01");
  await screen.findByRole("document", { name: "Reading column" });
  const planLinks = (root: HTMLElement) =>
    within(root).queryAllByRole("link").filter(hrefIs("/f/demo/engineering/first"));
  const opener = await screen.findByRole("button", { name: /^Intent is written down/ });
  expect(planLinks(opener.parentElement!)).toEqual([]);
  fireEvent.click(opener);
  let line = await holding("Covered by");
  while (line.parentElement && within(line).queryAllByRole("link").length === 0) {
    line = line.parentElement;
  }
  expect(planLinks(line)).not.toEqual([]);
  fireEvent.click(await screen.findByRole("button", { name: /^Each stage names/ }));
  await holding("Not covered by any plan");
});

it("a document's source draws its trace head and reading column under distinct keys", async () => {
  const errors = vi.spyOn(console, "error").mockImplementation(() => undefined);
  try {
    renderRoutes("/f/demo/source/product/demo/section_03.md");
    await screen.findByRole("button", { name: "Mark settled" });
    const keyWarnings = errors.mock.calls.filter((call) => /same key/.test(String(call[0])));
    expect(keyWarnings).toEqual([]);
  } finally {
    errors.mockRestore();
  }
});
