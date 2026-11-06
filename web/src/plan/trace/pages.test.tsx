import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import docFixture from "@/plan/api/fixtures/files-doc.json";
import traceFixture from "@/plan/api/fixtures/trace.json";
import { atlas, offer, renderPage } from "@/plan/feature/pages-kit";

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
  vi.stubGlobal("fetch", (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const { pathname } = new URL(String(input), "http://localhost");
    if (pathname === "/api/doc") {
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
  it("says there are no findings yet", async () => {
    served = { status: 200, body: EMPTY_TRACE };
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    expect(await screen.findByText(/No findings yet/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: /^Plan this/ })).toBeNull();
  });

  it("groups the ledger by run and review, with actions on open findings only", async () => {
    served = { status: 200, body: traceFixture };
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    const run = await screen.findByRole("heading", { name: "REVIEW-PLAN-first" });
    expect(run).toBeTruthy();
    const reviews = screen.getByRole("heading", { name: "Reviews" });
    expect(reviews).toBeTruthy();
    // F-1 is open and F-2 dismissed: one set of actions, and the reason shows on the dismissed one.
    expect(screen.getAllByRole("button", { name: /^Dismiss/ })).toHaveLength(1);
    const dismissed = screen.getByText("Rename the flag").closest("li")!;
    expect(within(dismissed).getByText("Dismissed: not needed")).toBeTruthy();
  });

  it("blocks Plan this with a visible reason when the feature has no follow-up offer", async () => {
    served = { status: 200, body: traceFixture };
    renderPage(<FindingsPage />, "/f/atlas/findings", atlas({}));
    const plan = await screen.findByRole("button", { name: /^Plan this/ });
    expect(plan.getAttribute("aria-disabled")).toBe("true");
    // One reason for both buttons that need the offer.
    expect(screen.getAllByText(/no Plan follow-ups/)).toHaveLength(1);
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
    expect(await screen.findByRole("heading", { name: "REVIEW-PLAN-first" })).toBeTruthy();
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
    fireEvent.click(await screen.findByRole("button", { name: /^Dismiss/ }));
    return screen.findByRole("textbox", { name: /reason/i });
  }

  it("is blocked, with its reason shown, until the ledger's hash is held", async () => {
    served = { status: 200, body: traceFixture };
    const reason = await openForm();
    fireEvent.change(reason, { target: { value: "duplicate" } });
    const confirm = screen.getByRole("button", { name: "Confirm dismissal" });
    expect(confirm.getAttribute("aria-disabled")).toBe("true");
    expect(screen.getByText("Reading the ledger")).toBeTruthy();
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

  it("takes focus when the form opens and gives it back to Dismiss when it closes", async () => {
    served = { status: 200, body: traceFixture };
    const reason = await openForm();
    expect(document.activeElement).toBe(reason);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(document.activeElement).toBe(screen.getByRole("button", { name: /^Dismiss/ })),
    );
  });
});

describe("the coverage page", () => {
  it("selects a clicked card, focuses it, and names the focus panel after it", async () => {
    served = { status: 200, body: traceFixture };
    renderPage(<CoveragePage />, "/f/atlas/coverage", atlas({}));
    const finding = await screen.findByRole("button", { name: /Rebuild ignores briefs/ });
    fireEvent.click(finding);
    expect(document.activeElement).toBe(finding);
    expect(screen.getByRole("complementary", { name: /Rebuild ignores briefs/ })).toBeTruthy();
  });
});
