import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import authoringFixture from "@/plan/api/fixtures/authoring.json";
import checklistFixture from "@/plan/api/fixtures/checklist.json";
import treeFixture from "@/plan/api/fixtures/files-tree.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import {
  authoringViewSchema,
  checklistViewSchema,
  serverInfoSchema,
  type ChecklistView,
  type OfferView,
} from "@/plan/api/schema";
import { registerOfferAction } from "@/plan/offers/actions";
import { routes } from "@/plan/router";
import { activeFeatureAtom, authoringAtom, projectAtom, serverInfoAtom } from "@/plan/state/atoms";

// The panel's desktop region, without its socket and transcript. Its registrations go with it, so
// the Plan follow-ups action is registered below.
vi.mock("@/plan/chat/panel", async (original) => ({
  ...(await original<typeof import("@/plan/chat/panel")>()),
  ChatPanel: () => <aside aria-label="Chat" />,
  ChatRegistrations: () => null,
}));

const WIDE_QUERY = "(min-width: 1024px)";
const FOLLOW_UPS = {
  id: "plan-follow-ups",
  kind: "plan_follow_ups",
  label: "Plan follow-ups",
  target: null,
};
const REVIEW_ONE = "doc/plans/REVIEW-PLAN-atlas-01.md";
const REVIEW_TWO = "doc/plans/atlas/archive/REVIEW-PLAN-atlas-02.md";

let calls: { offer: OfferView; seed: unknown }[] = [];
let unregister: (() => void)[] = [];

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function requestUrl(input: RequestInfo | URL): URL {
  const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
  return new URL(raw, "http://localhost");
}

/** The server: the file tree, else 404 (the page reads nothing but the checklist). */
function stubServer(): void {
  vi.stubGlobal("fetch", (input: RequestInfo | URL): Promise<Response> => {
    const url = requestUrl(input);
    if (url.pathname === "/api/tree") return Promise.resolve(json(treeFixture));
    return Promise.resolve(json({ error: "not_found", message: "no such page" }, 404));
  });
}

function stubMedia(): void {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: query === WIDE_QUERY,
    media: query,
    addEventListener() {},
    removeEventListener() {},
  }));
}

function donePlan(slug: string) {
  return {
    slug,
    path: `doc/plans/atlas/archive/DONE-PLAN-${slug}.md`,
    title: `Plan ${slug}`,
    state: "done",
    pressure_date: null,
    listed: true,
  };
}

function candidate(kind: "finding" | "question", text: string, review = REVIEW_ONE) {
  return { kind, stage: "Chat polish", stage_id: "chat-polish", text, review };
}

/** Two done plans with every patch over a feature that is ready and all done. */
function builtChecklist(patch: Record<string, unknown> = {}): ChecklistView {
  return checklistViewSchema.parse({
    ...checklistFixture.checklists[0]!,
    feature: "atlas",
    metaplans: [{ path: "doc/plans/atlas/index.md", title: "Atlas roadmap" }],
    plan_rows: [donePlan("atlas-01"), donePlan("atlas-02")],
    plans: { listed: 2, total: 2, written: 2, running: 0, done: 2 },
    untested: 0,
    current: { kind: "ready_all_done" },
    steps: checklistFixture.checklists[0]!.steps.map((step) => ({ ...step, mark: "done" })),
    primary: null,
    secondary: [FOLLOW_UPS],
    items: [],
    built: {
      reviews: [
        { slug: "atlas-01", path: REVIEW_ONE },
        { slug: "atlas-02", path: REVIEW_TWO },
      ],
      spec: [
        {
          stage: "Integration Verification",
          stage_id: "integration-verify",
          text: "Corrected section_01 to match the build",
          review: REVIEW_ONE,
        },
      ],
      candidates: [
        candidate("finding", "Settings statistics look bland"),
        candidate("question", "Should the footer keep its meters?", REVIEW_TWO),
      ],
    },
    ...patch,
  });
}

function renderRoutes(path: string, checklist: ChecklistView) {
  const store = createStore();
  initClient(store);
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...serverInfoFixture, read_only: [] }));
  store.set(authoringAtom, authoringViewSchema.parse({ ...authoringFixture, holder: null }));
  store.set(projectAtom, { checklists: [checklist], standalone: [], moves: [] });
  store.set(activeFeatureAtom, null);
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  return { router, store };
}

async function list(name: string): Promise<HTMLElement> {
  return screen.findByRole("list", { name });
}

function rowNaming(container: HTMLElement, text: string): HTMLElement {
  const row = within(container)
    .getAllByRole("listitem")
    .find((item) => item.textContent?.includes(text));
  if (!row) throw new Error(`no row names "${text}"`);
  return row;
}

beforeEach(() => {
  window.localStorage.clear();
  calls = [];
  stubMedia();
  stubServer();
  unregister = [
    registerOfferAction("plan_follow_ups", (offer, seed) => {
      calls.push({ offer, seed });
    }),
  ];
});

afterEach(() => {
  cleanup();
  unregister.forEach((remove) => remove());
  vi.unstubAllGlobals();
});

describe("the Built page", () => {
  it("lists the done plans with their run reviews, the spec check and the candidates", async () => {
    renderRoutes("/f/atlas/built", builtChecklist());

    const plans = await list("Done plans");
    const first = rowNaming(plans, "atlas-01");
    expect(within(first).getByRole("link", { name: "atlas-01" }).getAttribute("href")).toBe(
      "/f/atlas/engineering/atlas-01",
    );
    expect(first.textContent).toContain("Plan atlas-01");
    expect(within(first).getByRole("link", { name: "Run review" }).getAttribute("href")).toBe(
      "/files/plans/REVIEW-PLAN-atlas-01.md",
    );
    const second = rowNaming(plans, "atlas-02");
    expect(within(second).getByRole("link", { name: "Run review" }).getAttribute("href")).toBe(
      "/f/atlas/source/plans/atlas/archive/REVIEW-PLAN-atlas-02.md",
    );

    const spec = await list("Spec check");
    const change = rowNaming(spec, "Corrected section_01");
    expect(change.textContent).toContain("Integration Verification");
    expect(
      within(change).getByRole("link", { name: "REVIEW-PLAN-atlas-01.md" }).getAttribute("href"),
    ).toBe("/files/plans/REVIEW-PLAN-atlas-01.md");

    const candidates = await list("Candidate follow-ups");
    expect(rowNaming(candidates, "Settings statistics").textContent).toContain("Open finding");
    const question = rowNaming(candidates, "Should the footer");
    expect(question.textContent).toContain("Open question");
    expect(question.textContent).toContain("Chat polish");
    expect(within(question).getByRole("link", { name: "REVIEW-PLAN-atlas-02.md" })).toBeTruthy();
    expect(within(candidates).getAllByRole("button", { name: "Plan this" })).toHaveLength(2);
  });

  it("reads one empty sentence per list with nothing in it", async () => {
    renderRoutes(
      "/f/atlas/built",
      builtChecklist({ built: { reviews: [], spec: [], candidates: [] } }),
    );

    expect(await screen.findByText("The runs recorded no change to the spec.")).toBeTruthy();
    expect(screen.getByText("The run reviews list no open findings or questions.")).toBeTruthy();
    expect(screen.queryByRole("list", { name: "Spec check" })).toBeNull();
    expect(screen.queryByRole("list", { name: "Candidate follow-ups" })).toBeNull();
  });

  it("reads No run review on a done plan whose review was not written", async () => {
    const built = { reviews: [{ slug: "atlas-01", path: REVIEW_ONE }], spec: [], candidates: [] };
    renderRoutes("/f/atlas/built", builtChecklist({ built }));

    const plans = await list("Done plans");
    const second = rowNaming(plans, "atlas-02");
    expect(second.textContent).toContain("No run review");
    expect(within(second).queryByRole("link", { name: "Run review" })).toBeNull();
    const first = rowNaming(plans, "atlas-01");
    expect(within(first).getByRole("link", { name: "Run review" })).toBeTruthy();
  });

  it("seeds Plan this with the question and its source", async () => {
    renderRoutes("/f/atlas/built", builtChecklist());

    const candidates = await list("Candidate follow-ups");
    fireEvent.click(
      within(rowNaming(candidates, "Should the footer")).getByRole("button", { name: "Plan this" }),
    );

    expect(calls).toHaveLength(1);
    expect(calls[0]!.offer.kind).toBe("plan_follow_ups");
    expect(calls[0]!.seed).toBe(
      "Should the footer keep its meters? (open question from REVIEW-PLAN-atlas-02.md, stage chat-polish)",
    );
  });

  it("describes each Plan this button by its candidate's text", async () => {
    renderRoutes("/f/atlas/built", builtChecklist());

    const candidates = await list("Candidate follow-ups");
    const row = rowNaming(candidates, "Should the footer");
    const button = within(row).getByRole("button", { name: "Plan this" });
    const ids = (button.getAttribute("aria-describedby") ?? "").split(" ");
    const target = document.getElementById(ids[ids.length - 1]!);
    expect(target?.textContent).toContain("Should the footer keep its meters?");
    expect(row.contains(target)).toBe(true);
  });

  it("blocks Plan follow-ups while the box is blank, then sends the trimmed text and clears it", async () => {
    renderRoutes("/f/atlas/built", builtChecklist());

    const box = await screen.findByLabelText("What else needs doing?");
    const button = screen.getByRole("button", { name: "Plan follow-ups" });
    expect(button.getAttribute("aria-disabled")).toBe("true");
    expect(button.getAttribute("data-look")).toBe("primary");
    expect(screen.getByText("Describe the follow-up")).toBeTruthy();
    fireEvent.click(button);
    expect(calls).toEqual([]);

    fireEvent.change(box, { target: { value: "  Tidy the footer meters \n" } });
    expect(button.getAttribute("aria-disabled")).toBeNull();
    expect(screen.queryByText("Describe the follow-up")).toBeNull();
    fireEvent.click(button);

    expect(calls).toHaveLength(1);
    expect(calls[0]!.seed).toBe("Tidy the footer meters");
    expect((box as HTMLTextAreaElement).value).toBe("");
    expect(button.getAttribute("aria-disabled")).toBe("true");
  });

  it("links Start it on New work to /new with the feature as the follow-up", async () => {
    const { router } = renderRoutes("/f/atlas/built", builtChecklist());

    const link = await screen.findByRole("link", { name: "Start it on New work" });
    expect(link.getAttribute("href")).toBe("/new");
    fireEvent.click(link);
    await waitFor(() => expect(router.state.location.pathname).toBe("/new"));
    expect(router.state.location.state).toEqual({ followUp: "atlas" });
  });

  it("blocks both actions under the authoring block and shows its reason", async () => {
    const { store } = renderRoutes("/f/atlas/built", builtChecklist());
    // The fixture's own read-only entry: a missing prerequisite blocks authoring.
    store.set(serverInfoAtom, serverInfoSchema.parse(serverInfoFixture));

    const candidates = await list("Candidate follow-ups");
    const planThis = within(candidates).getAllByRole("button", { name: "Plan this" })[0]!;
    await waitFor(() => expect(planThis.getAttribute("aria-disabled")).toBe("true"));
    fireEvent.click(planThis);
    expect(calls).toEqual([]);
    const primary = screen.getByRole("button", { name: "Plan follow-ups" });
    expect(primary.getAttribute("aria-disabled")).toBe("true");
    expect(screen.getAllByText("Read-only").length).toBeGreaterThan(0);
  });

  it("sends an unfinished feature from /built to where it lands", async () => {
    const { router } = renderRoutes(
      "/f/atlas/built",
      builtChecklist({
        plan_rows: [donePlan("atlas-01"), { ...donePlan("atlas-02"), state: "written" }],
        plans: { listed: 2, total: 2, written: 2, running: 0, done: 1 },
        current: { kind: "ready" },
      }),
    );

    await waitFor(() => expect(router.state.location.pathname).toBe("/f/atlas/product"));
    expect(screen.queryByRole("heading", { level: 1, name: "Built" })).toBeNull();
  });

  it("draws no Plan this and no free-text box without the Plan follow-ups offer", async () => {
    renderRoutes("/f/atlas/built", builtChecklist({ secondary: [] }));

    const candidates = await list("Candidate follow-ups");
    expect(within(candidates).queryByRole("button", { name: "Plan this" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Plan follow-ups" })).toBeNull();
    expect(screen.queryByLabelText("What else needs doing?")).toBeNull();
    expect(screen.getByRole("link", { name: "Start it on New work" })).toBeTruthy();
  });

  it("draws no Plan this while no action is registered for the offer", async () => {
    unregister.forEach((remove) => remove());
    unregister = [];
    renderRoutes("/f/atlas/built", builtChecklist());

    const candidates = await list("Candidate follow-ups");
    expect(within(candidates).queryByRole("button", { name: "Plan this" })).toBeNull();
  });
});
