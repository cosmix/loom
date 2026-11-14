import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import docFixture from "@/plan/api/fixtures/files-doc.json";
import traceFixture from "@/plan/api/fixtures/trace.json";
import type { TraceView } from "@/plan/api/schema-trace";
import { atlas, offer, renderPage } from "@/plan/feature/pages-kit";
import { registerOfferAction } from "@/plan/offers/actions";

import { CoveragePage } from "./coverage-page";
import { FindingsPage } from "./findings-page";

const EMPTY_TRACE = {
  documents: [],
  headings: [],
  entries: [],
  findings: [],
  gaps: [],
  frontier: [],
  problems: [],
};

type Served = { status: number; body: unknown } | "pending";
let served: Served = "pending";
/** The hash `/api/doc` answers for the ledger; null leaves the request unanswered. */
let docHash: string | null = null;
let posts: Record<string, unknown>[] = [];
let docFetches = 0;

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

beforeEach(() => {
  served = "pending";
  docHash = null;
  posts = [];
  docFetches = 0;
  vi.stubGlobal("fetch", (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const { pathname } = new URL(String(input), "http://localhost");
    if (pathname === "/api/doc") {
      docFetches += 1;
      return docHash === null
        ? new Promise<Response>(() => {})
        : Promise.resolve(json(200, { ...docFixture, hash: docHash }));
    }
    if (pathname === "/api/findings/status") {
      posts.push(JSON.parse(String(init?.body)) as Record<string, unknown>);
      return Promise.resolve(
        json(200, { result: "saved", hash: "sha256:after", validation: null }),
      );
    }
    if (pathname !== "/api/trace/atlas") return Promise.resolve(json(404, { error: "not_found" }));
    return served === "pending"
      ? new Promise<Response>(() => {})
      : Promise.resolve(json(served.status, served.body));
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe.each([
  ["coverage", <CoveragePage key="c" />, "the coverage"],
  ["findings", <FindingsPage key="f" />, "the findings"],
])("the %s page before it has a trace", (page, element, noun) => {
  it("says it is loading while the trace is on its way", () => {
    renderPage(element, `/f/atlas/${page}`, atlas({}));
    expect(screen.getByRole("status", { name: `Loading ${noun}` })).toBeTruthy();
  });

  it("says why when the trace cannot be loaded, and offers another try", async () => {
    served = { status: 500, body: { error: "boom", message: "The trace is unavailable" } };
    renderPage(element, `/f/atlas/${page}`, atlas({}));
    expect(await screen.findByText("The trace is unavailable")).toBeTruthy();
    expect(screen.getByText(`Could not load ${noun}`)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Try again" })).toBeTruthy();
  });
});

describe("the coverage page with nothing to map", () => {
  it("says what comes first, and draws no map", async () => {
    served = { status: 200, body: EMPTY_TRACE };
    renderPage(<CoveragePage />, "/f/atlas/coverage", atlas({}));
    expect(await screen.findByText(/Nothing to map yet/)).toBeTruthy();
    expect(screen.queryByRole("complementary")).toBeNull();
  });
});

describe("the findings page with no ledger", () => {
  it("says there are no findings yet, and offers no address run", async () => {
    served = { status: 200, body: EMPTY_TRACE };
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    expect(await screen.findByText(/No findings yet/)).toBeTruthy();
    expect(screen.queryByText("Address the findings")).toBeNull();
  });

  it("groups the ledger by run and review, with Dismiss on open findings only", async () => {
    served = { status: 200, body: traceFixture };
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    expect(await screen.findByRole("heading", { name: "Run of first" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Reviews" })).toBeTruthy();
    // F-1 is open and verified, F-2 dismissed.
    expect(screen.getByText("1 to address · 1 dismissed")).toBeTruthy();
    const dismissals = screen.getAllByRole("button", { name: /^Dismiss/ });
    expect(dismissals.map((button) => button.getAttribute("aria-label"))).toEqual(["Dismiss F-1"]);
    const dismissed = screen.getByText("Rename the flag").closest("li")!;
    expect(within(dismissed).getByText("Dismissed: not needed")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /^Plan this/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /^Fold into/ })).toBeNull();
  });

  it("shows each card's check, its quote and what an addressed finding changed", async () => {
    const trace = structuredClone(traceFixture) as TraceView;
    const [verified] = trace.findings;
    const raw = structuredClone(verified!);
    raw.finding = { ...raw.finding, id: "F-3", title: "Raw item", evidence: null, severity: null };
    raw.check = { kind: "unverified", duplicate_of: null };
    const done = structuredClone(verified!);
    done.finding = {
      ...done.finding,
      id: "F-4",
      title: "Heading one",
      status: { kind: "addressed", detail: "section_01#one" },
    };
    done.changed = [{ path: "doc/product/demo/section_01.md", anchor: "one" }];
    trace.findings.push(raw, done);
    served = { status: 200, body: trace };
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    const open = (await screen.findByText("Rebuild ignores briefs")).closest("li")!;
    expect(within(open).getByText("Open · medium")).toBeTruthy();
    expect(within(open).getByText("Text two names the budget.")).toBeTruthy();
    const unverified = screen.getByText("Raw item").closest("li")!;
    expect(unverified.getAttribute("data-state")).toBe("unverified");
    expect(within(unverified).getByText(/^Unverified/)).toBeTruthy();
    expect(screen.getByText("1 to address · 1 to verify · 1 addressed · 1 dismissed")).toBeTruthy();
    const addressed = screen.getByText("Heading one").closest("li")!;
    expect(addressed.getAttribute("data-line")).toBe("true");
    const changed = within(addressed).getByRole("group", { name: "Changed" });
    expect(within(changed).getByRole("link", { name: "One" })).toBeTruthy();
  });

  it("draws a duplicate as a line that links to the finding it repeats", async () => {
    const trace = structuredClone(traceFixture) as TraceView;
    trace.findings[0]!.check = { kind: "duplicate", duplicate_of: "F-3" };
    served = { status: 200, body: trace };
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    const line = (await screen.findByText("Rebuild ignores briefs")).closest("li")!;
    expect(line.getAttribute("data-line")).toBe("true");
    expect(within(line).getByRole("link", { name: "F-3" }).getAttribute("href")).toMatch(
      /#finding-F-3$/,
    );
    expect(within(line).queryByRole("button")).toBeNull();
  });
});

describe("Address the findings", () => {
  it("starts the address offer for the feature from the page head", async () => {
    served = { status: 200, body: traceFixture };
    const started: unknown[] = [];
    const unregister = registerOfferAction("address_findings", (run) => started.push(run));
    try {
      renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
      fireEvent.click(await screen.findByRole("button", { name: "Address the findings" }));
      expect(started).toEqual([
        {
          id: "address-findings",
          kind: "address_findings",
          label: "Address the findings",
          target: null,
          feature: "atlas",
        },
      ]);
    } finally {
      unregister();
    }
  });

  it("is not offered while no finding is open and verified", async () => {
    const trace = structuredClone(traceFixture) as TraceView;
    trace.findings[0]!.check = { kind: "stale", duplicate_of: null };
    served = { status: 200, body: trace };
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    await screen.findByRole("heading", { name: "Reviews" });
    expect(screen.queryByText("Address the findings")).toBeNull();
  });
});

describe("reading the ledger's hash", () => {
  it("does not fetch the ledger when the trace has no findings", async () => {
    served = { status: 200, body: EMPTY_TRACE };
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    await screen.findByText(/No findings yet/);
    expect(docFetches).toBe(0);
  });

  it("fetches the ledger when the trace has findings", async () => {
    served = { status: 200, body: traceFixture };
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    await screen.findByRole("heading", { name: "Reviews" });
    await waitFor(() => expect(docFetches).toBe(1));
  });
});

describe("the findings page header", () => {
  it("draws Review the feature although the trace cannot be loaded", async () => {
    served = { status: 500, body: { error: "boom", message: "The trace is unavailable" } };
    const checklist = atlas({ secondary: [offer("review_feature", "Review the feature")] });
    renderPage(<FindingsPage />, "/f/atlas/findings", checklist);
    expect(await screen.findByText("The trace is unavailable")).toBeTruthy();
    // No offer handler is registered in a page-only render, so the offer shows as its label.
    expect(screen.getByText("Review the feature")).toBeTruthy();
  });

  it("survives an address whose fragment is not a valid escape", async () => {
    served = { status: 200, body: traceFixture };
    renderPage(<FindingsPage />, "/f/atlas/findings#%E0", atlas({}));
    expect(await screen.findByRole("heading", { name: "Run of first" })).toBeTruthy();
  });

  it("marks the finding the address names", async () => {
    served = { status: 200, body: traceFixture };
    renderPage(<FindingsPage />, "/f/atlas/findings#finding-F-1", atlas({}));
    const addressed = (await screen.findByText("Rebuild ignores briefs")).closest("li")!;
    expect(addressed.getAttribute("data-addressed")).toBe("true");
    const other = screen.getByText("Rename the flag").closest("li")!;
    expect(other.hasAttribute("data-addressed")).toBe(false);
  });
});

describe("dismissing a finding", () => {
  async function openForm(): Promise<HTMLElement> {
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    fireEvent.click(await screen.findByRole("button", { name: "Dismiss F-1" }));
    return screen.findByRole("textbox", { name: /reason/i });
  }

  it("is blocked, with its reason shown, until the ledger's hash is held", async () => {
    served = { status: 200, body: traceFixture };
    const reason = await openForm();
    fireEvent.change(reason, { target: { value: "duplicate" } });
    const confirm = screen.getByRole("button", { name: "Confirm dismissal" });
    expect(confirm.getAttribute("aria-disabled")).toBe("true");
    // The dismissed card's Reopen is blocked for the same reason, so the reason shows on both.
    expect(screen.getAllByText("Reading the ledger").length).toBeGreaterThan(0);
    fireEvent.click(confirm);
    expect(posts).toEqual([]);
  });

  it("sends the hash read with the trace on screen, not one read at click time", async () => {
    served = { status: 200, body: traceFixture };
    docHash = "sha256:shown";
    const reason = await openForm();
    fireEvent.change(reason, { target: { value: "duplicate" } });
    const confirm = screen.getByRole("button", { name: "Confirm dismissal" });
    await waitFor(() => expect(confirm.getAttribute("aria-disabled")).toBeNull());
    // The ledger moves on after the page read it; the server's hash check must see the old one.
    docHash = "sha256:moved";
    fireEvent.click(confirm);
    await waitFor(() => expect(posts).toHaveLength(1));
    expect(posts[0]).toMatchObject({ id: "F-1", hash: "sha256:shown" });
  });

  it("asks for a reason first, and takes the confirm once one is written", async () => {
    served = { status: 200, body: traceFixture };
    docHash = "sha256:shown";
    const reason = await openForm();
    const confirm = screen.getByRole("button", { name: "Confirm dismissal" });
    expect(screen.getByText("Write a reason first")).toBeTruthy();
    expect(confirm.getAttribute("aria-disabled")).toBe("true");
    fireEvent.change(reason, { target: { value: "duplicate" } });
    await waitFor(() => expect(confirm.getAttribute("aria-disabled")).toBeNull());
    expect(screen.queryByText("Write a reason first")).toBeNull();
  });

  it("takes focus when the form opens and gives it back to Dismiss when it closes", async () => {
    served = { status: 200, body: traceFixture };
    const reason = await openForm();
    expect(document.activeElement).toBe(reason);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(document.activeElement).toBe(screen.getByRole("button", { name: "Dismiss F-1" })),
    );
  });
});

describe("after a dismissal is saved", () => {
  it("moves focus to the finding's heading, since the actions are gone", async () => {
    served = { status: 200, body: traceFixture };
    docHash = "sha256:shown";
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    fireEvent.click(await screen.findByRole("button", { name: "Dismiss F-1" }));
    fireEvent.change(await screen.findByRole("textbox", { name: /reason/i }), {
      target: { value: "duplicate" },
    });
    const confirm = screen.getByRole("button", { name: "Confirm dismissal" });
    await waitFor(() => expect(confirm.getAttribute("aria-disabled")).toBeNull());
    fireEvent.click(confirm);
    const heading = screen.getByRole("heading", { name: "Rebuild ignores briefs" });
    await waitFor(() => expect(document.activeElement).toBe(heading));
  });
});
