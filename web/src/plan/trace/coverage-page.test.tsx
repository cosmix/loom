import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import traceFixture from "@/plan/api/fixtures/trace.json";
import type { TraceView } from "@/plan/api/schema-trace";
import { Gaps } from "@/plan/feature/gaps";
import { atlas, renderPage } from "@/plan/feature/pages-kit";

import { CoveragePage } from "./coverage-page";

// The coverage page as a reader meets it: the tiles, the rows and what they open to. The model
// is `coverage-model.test.ts`; the findings page and its actions are `pages.test.tsx`.

let served: unknown = traceFixture;

beforeEach(() => {
  served = traceFixture;
  vi.stubGlobal("fetch", (input: RequestInfo | URL): Promise<Response> => {
    const { pathname } = new URL(String(input), "http://localhost");
    const found = pathname === "/api/trace/atlas";
    // A document request (the vision row quoted under Why) is never answered.
    if (pathname === "/api/doc") return new Promise<Response>(() => {});
    return Promise.resolve(
      new Response(JSON.stringify(found ? served : { error: "not_found" }), {
        status: found ? 200 : 404,
        headers: { "content-type": "application/json" },
      }),
    );
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

async function drawn(trace: unknown = traceFixture, path = "/f/atlas/coverage") {
  served = trace;
  renderPage(<CoveragePage />, path, atlas({}));
  await screen.findByRole("heading", { name: "Alpha" });
}

const S01 = "doc/product/demo/section_01.md";

/** The fixture trace with section 01 gaining heading Five, which nothing covers. */
function uncoveredTrace(): TraceView {
  const trace = structuredClone(traceFixture) as TraceView;
  trace.headings.push({ path: S01, anchor: "five", title: "Five", covered_by: [], findings: [] });
  return trace;
}

const toggles = () =>
  screen.queryAllByRole("button").filter((button) => button.hasAttribute("aria-expanded"));
const toggle = (name: RegExp) => toggles().find((button) => name.test(button.textContent ?? ""))!;

describe("the coverage page tiles", () => {
  it("count the headings and filter the rows to the one picked", async () => {
    await drawn(uncoveredTrace());
    // Each tile is named by its visible count and word. Five headings, one of them uncovered, one
    // with an open finding, none changed.
    const all = await screen.findByRole("button", { name: "5 All headings" });
    const tiles = ["1 Not covered", "0 Changed since covered", "1 With open findings"];
    for (const name of tiles) expect(screen.getByRole("button", { name })).toBeTruthy();
    expect(all.getAttribute("aria-pressed")).toBe("true");
    expect(toggles()).toHaveLength(5);

    fireEvent.click(screen.getByRole("button", { name: "1 Not covered" }));
    expect(all.getAttribute("aria-pressed")).toBe("false");
    expect(toggles().map((button) => button.textContent)).toEqual(["Five"]);

    fireEvent.click(screen.getByRole("button", { name: "1 With open findings" }));
    expect(toggles().map((button) => button.textContent)).toEqual(["Two"]);

    fireEvent.click(screen.getByRole("button", { name: "0 Changed since covered" }));
    expect(toggles()).toEqual([]);
    // In a live region, so the emptied filter is announced.
    const empty = screen.getByText("No heading matches this filter.");
    expect(empty.closest('[role="status"]')).toBeTruthy();
  });
});

describe("the coverage page rows", () => {
  it("share a column header and say each heading's state in words", async () => {
    await drawn(uncoveredTrace());
    expect(screen.getAllByText("Covered by").length).toBeGreaterThan(0);
    const row = toggle(/^Five$/).closest("li")!;
    expect(within(row).getByText("Not covered")).toBeTruthy();
    expect(within(row).getByText("Nothing yet")).toBeTruthy();
    const two = toggle(/^Two$/).closest("li")!;
    expect(within(two).getByRole("link", { name: "first" }).getAttribute("href")).toBe(
      "/f/atlas/engineering/first",
    );
    expect(within(two).getByRole("link", { name: "build" })).toBeTruthy();
    expect(within(two).getByText("1 open")).toBeTruthy();
    expect(within(two).getByText("Planned")).toBeTruthy();
    expect(screen.getByText("2 of 3 covered")).toBeTruthy();
  });

  it("open on their toggle to Why, How and Findings, and close again", async () => {
    await drawn();
    const two = toggle(/^Two$/);
    expect(two.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByRole("heading", { name: "Why" })).toBeNull();
    fireEvent.click(two);
    expect(two.getAttribute("aria-expanded")).toBe("true");
    const detail = document.getElementById(two.getAttribute("aria-controls")!)!;
    expect(
      within(detail)
        .getAllByRole("heading")
        .map((h) => h.textContent),
    ).toEqual(["Why", "How", "Findings"]);
    expect(
      within(detail)
        .getByRole("link", { name: /Rebuild ignores briefs/ })
        .getAttribute("href"),
    ).toBe("/f/atlas/findings#finding-F-1");
    expect(within(detail).getByText("Stage")).toBeTruthy();
    fireEvent.click(two);
    expect(two.getAttribute("aria-expanded")).toBe("false");
    expect(two.hasAttribute("aria-controls")).toBe(false);
  });

  it("say None open for a heading without an open finding", async () => {
    await drawn();
    fireEvent.click(toggle(/^One$/));
    expect(screen.getByText("None open.")).toBeTruthy();
  });

  it("move focus with the arrow keys and stop at the ends", async () => {
    await drawn();
    const [first, second] = toggles();
    first!.focus();
    fireEvent.keyDown(first!, { key: "ArrowDown" });
    expect(document.activeElement).toBe(second);
    fireEvent.keyDown(second!, { key: "ArrowUp" });
    expect(document.activeElement).toBe(first);
    fireEvent.keyDown(first!, { key: "ArrowUp" });
    expect(document.activeElement).toBe(first);
    const last = toggles().at(-1)!;
    last.focus();
    fireEvent.keyDown(last, { key: "ArrowDown" });
    expect(document.activeElement).toBe(last);
  });

  it("leave the arrow keys alone when the focus is not on a row toggle", async () => {
    await drawn();
    const tile = screen.getByRole("button", { name: /^\d+ Not covered$/ });
    tile.focus();
    expect(fireEvent.keyDown(tile, { key: "ArrowDown" })).toBe(true);
    expect(document.activeElement).toBe(tile);
  });

  it("show a section that is not written as one quiet line", async () => {
    await drawn();
    const gamma = screen.getByRole("heading", { name: "Gamma" }).closest("section")!;
    expect(within(gamma).getByText("Not written yet.")).toBeTruthy();
    expect(within(gamma).queryAllByRole("button")).toEqual([]);
  });

  it("open the heading the address names", async () => {
    await drawn(undefined, "/f/atlas/coverage#two");
    await waitFor(() => expect(toggle(/^Two$/).getAttribute("aria-expanded")).toBe("true"));
    expect(toggle(/^One$/).getAttribute("aria-expanded")).toBe("false");
  });

  it("open the heading a Coverage link names as section and anchor", async () => {
    await drawn(undefined, "/f/atlas/coverage#section_01%3Atwo");
    await waitFor(() => expect(toggle(/^Two$/).getAttribute("aria-expanded")).toBe("true"));
    expect(toggle(/^One$/).getAttribute("aria-expanded")).toBe("false");
  });

  it("flag a plan whose spec changed, with the way to see what changed", async () => {
    const trace = structuredClone(traceFixture) as TraceView;
    Object.assign(
      trace.documents.find((doc) => doc.slug === "first")!,
      {
        settle: "upstream_changed",
      },
    );
    await drawn(trace);
    expect(within(toggle(/^One$/).closest("li")!).getByText("Changed since covered")).toBeTruthy();
    fireEvent.click(toggle(/^One$/));
    const note = screen.getByRole("note");
    expect(note.textContent).toMatch(/changed since you settled it/);
    expect(within(note).getByRole("link", { name: "See what changed" })).toBeTruthy();
  });
});

describe("the entry links", () => {
  // The way to Coverage is the spine's well at the foot of the Product spec group (spine-instrument.tsx).
  it.each(["product", "engineering"] as const)(
    "draw no Coverage link of their own on the %s overview",
    (scope) => {
      renderPage(<Gaps scope={scope} />, `/f/atlas/${scope}`, atlas({}));
      expect(screen.queryByRole("link", { name: /Coverage/ })).toBeNull();
    },
  );
});
