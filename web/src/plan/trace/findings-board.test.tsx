import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import docFixture from "@/plan/api/fixtures/files-doc.json";
import traceFixture from "@/plan/api/fixtures/trace.json";
import type { FindingCheckKind, TraceFinding, TraceView } from "@/plan/api/schema-trace";
import { atlas, offer, renderPage } from "@/plan/feature/pages-kit";

import { FindingsPage } from "./findings-page";

// The board: which lane each state lands in, the lane counts and copy, a card opening its finding
// in a dialog that traps focus, closes on Escape and gives focus back, a `#finding-<id>` address
// opening a finding, and the quiet board when nothing waits.

let served: TraceView = traceFixture as TraceView;

function json(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

type Kind = TraceFinding["finding"]["status"]["kind"];

function finding(id: string, kind: Kind, check: FindingCheckKind = "verified"): TraceFinding {
  const [first] = (traceFixture as TraceView).findings;
  return {
    finding: {
      ...structuredClone(first!.finding),
      id,
      title: `Title of ${id}`,
      status: { kind, detail: kind === "open" || kind === "addressed" ? null : "because" },
    },
    about: structuredClone(first!.about),
    check: { kind: check, duplicate_of: check === "duplicate" ? "F-1" : null },
    changed: kind === "addressed" ? structuredClone(first!.about) : [],
  };
}

/** One finding of every card state: the lanes' contents are fixed by it. */
function everyState(): TraceView {
  const view = structuredClone(traceFixture) as TraceView;
  view.findings = [
    finding("F-1", "open"),
    finding("F-2", "open"),
    finding("F-3", "unaddressed"),
    finding("F-4", "open", "unverified"),
    finding("F-5", "open", "stale"),
    finding("F-6", "addressed"),
    finding("F-7", "dismissed"),
    finding("F-8", "open", "duplicate"),
  ];
  return view;
}

beforeEach(() => {
  served = everyState();
  vi.stubGlobal("fetch", (input: RequestInfo | URL): Promise<Response> => {
    const { pathname } = new URL(String(input), "http://localhost");
    if (pathname === "/api/doc") return Promise.resolve(json({ ...docFixture, hash: "sha256:a" }));
    if (pathname === "/api/trace/atlas") return Promise.resolve(json(served));
    return Promise.resolve(json({ error: "not_found" }));
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function lane(name: RegExp): HTMLElement {
  return screen.getByRole("region", { name });
}

const ids = (region: HTMLElement) =>
  within(region)
    .queryAllByRole("listitem")
    .map((item) => item.id);

describe("the board's lanes", () => {
  it("draws four lanes in the order a finding moves, each with its count", async () => {
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    await screen.findByRole("region", { name: /^Yours to decide/ });
    const names = screen.getAllByRole("heading", { level: 2 }).map((head) => head.textContent);
    expect(names).toEqual(["To verify 2", "Yours to decide 3", "Addressed 1", "Set aside 2"]);
  });

  it("puts each state in its lane, the finding waiting longest on the reader first", async () => {
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    await screen.findByRole("region", { name: /^Yours to decide/ });
    expect(ids(lane(/^To verify/))).toEqual(["finding-F-4", "finding-F-5"]);
    // The open ones in ledger order, then the one an address run could not fix.
    expect(ids(lane(/^Yours to decide/))).toEqual(["finding-F-1", "finding-F-2", "finding-F-3"]);
    expect(ids(lane(/^Addressed/))).toEqual(["finding-F-6"]);
    expect(ids(lane(/^Set aside/))).toEqual(["finding-F-7", "finding-F-8"]);
  });

  it("names the reason an address run could not fix a finding on its card", async () => {
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    const failed = (await screen.findByText("Title of F-3")).closest("li")!;
    expect(within(failed).getByText("Could not address")).toBeTruthy();
    expect(within(failed).getByText("because")).toBeTruthy();
  });

  it("owns each action in the lane it acts on", async () => {
    const checklist = atlas({ secondary: [offer("review_feature", "Review the feature")] });
    renderPage(<FindingsPage />, "/f/atlas/findings", checklist);
    await screen.findByRole("region", { name: /^Yours to decide/ });
    expect(within(lane(/^To verify/)).getByText("Review the feature")).toBeTruthy();
    expect(within(lane(/^Yours to decide/)).getByText("Address the findings")).toBeTruthy();
    // The head draws the offer only before the board exists.
    expect(screen.getAllByText("Review the feature")).toHaveLength(1);
  });
});

describe("a card's dialog", () => {
  it("opens on a press, traps focus, closes on Escape and gives focus back to the card", async () => {
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    const card = await screen.findByRole("button", { name: /Title of F-1/ });
    card.focus();
    fireEvent.click(card);
    const dialog = await screen.findByRole("dialog", { name: "Title of F-1" });
    expect(dialog.contains(document.activeElement)).toBe(true);
    expect(within(dialog).getByText("in Yours to decide")).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: "Dismiss F-1" })).toBeTruthy();
    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(card));
  });

  it("closes from its Close button", async () => {
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    fireEvent.click(await screen.findByRole("button", { name: /Title of F-6/ }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("in Addressed")).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("offers each state only the actions it has", async () => {
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    const actions = async (id: string) => {
      fireEvent.click(await screen.findByRole("button", { name: new RegExp(`Title of ${id}`) }));
      const dialog = await screen.findByRole("dialog");
      const names = within(dialog)
        .getAllByRole("button")
        .map((button) => button.getAttribute("aria-label") ?? button.textContent);
      fireEvent.keyDown(dialog, { key: "Escape" });
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      return names;
    };
    expect(await actions("F-4")).toEqual(["Close", "Dismiss F-4"]);
    expect(await actions("F-3")).toEqual(["Close", "Reopen F-3"]);
    expect(await actions("F-7")).toEqual(["Close", "Reopen F-7"]);
    expect(await actions("F-6")).toEqual(["Close"]);
  });

  it("shows a missing quote as such and a stale one as no longer in its passage", async () => {
    served.findings[3]!.finding.evidence = null;
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    fireEvent.click(await screen.findByRole("button", { name: /Title of F-4/ }));
    expect(await screen.findByText("No evidence yet")).toBeTruthy();
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    fireEvent.click(screen.getByRole("button", { name: /Title of F-5/ }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/The quote is no longer in/)).toBeTruthy();
  });
});

describe("a finding's address", () => {
  it("opens the finding the fragment names", async () => {
    renderPage(<FindingsPage />, "/f/atlas/findings#finding-F-6", atlas({}));
    const dialog = await screen.findByRole("dialog", { name: "Title of F-6" });
    expect(within(dialog).getByText("in Addressed")).toBeTruthy();
  });

  it("opens the finding a duplicate's link names", async () => {
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    fireEvent.click(await screen.findByRole("button", { name: /Title of F-8/ }));
    const dialog = await screen.findByRole("dialog", { name: "Title of F-8" });
    fireEvent.click(within(dialog).getByRole("link", { name: "F-1" }));
    await screen.findByRole("dialog", { name: "Title of F-1" });
  });

  it("opens nothing for a fragment that names no finding", async () => {
    renderPage(<FindingsPage />, "/f/atlas/findings#finding-F-99", atlas({}));
    await screen.findByRole("region", { name: /^Yours to decide/ });
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("a board with nothing waiting", () => {
  it("says so in the lede and in the two lanes that wait on the reader", async () => {
    served.findings = [finding("F-1", "addressed"), finding("F-2", "dismissed")];
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    await screen.findByRole("region", { name: /^Addressed/ });
    expect(screen.getByText(/^Nothing waits for you\. Every problem/)).toBeTruthy();
    expect(within(lane(/^Yours to decide/)).getByText("Nothing waits for you.")).toBeTruthy();
    expect(within(lane(/^To verify/)).getByText("Every finding has a checked quote.")).toBeTruthy();
    expect(screen.queryByText("Address the findings")).toBeNull();
    expect(ids(lane(/^Addressed/))).toEqual(["finding-F-1"]);
  });
});
