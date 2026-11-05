import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { act } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import {
  atlas,
  offer,
  planRow,
  renderPage,
  runState,
  runStrip,
  stripSocket,
} from "@/plan/feature/pages-kit";
import { RunPage } from "@/plan/feature/run-page";
import { registerOfferAction } from "@/plan/offers/actions";
import { runDialogAtom } from "@/plan/offers/actions-run";
import { fetchRunState } from "@/plan/run/use-run";

const PATH = "/f/atlas/run";
const ONE = "doc/plans/atlas/PLAN-one.md";
const TWO = "doc/plans/atlas/PLAN-two.md";
const handler = vi.fn();
let unregister: (() => void)[] = [];

beforeEach(() => {
  handler.mockReset();
  unregister = [registerOfferAction("plan_follow_ups", handler)];
});

afterEach(() => {
  cleanup();
  unregister.forEach((remove) => remove());
  vi.unstubAllGlobals();
});

/** The server's answer to `/api/run/state`, read afresh on each request. */
function serveRunState(answer: () => Response): void {
  vi.stubGlobal("fetch", () => Promise.resolve(answer()));
}
const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });

const written = atlas({ plan_rows: [planRow("one", "written"), planRow("two", "written")] });
const start = () => screen.getAllByRole("button", { name: "Start a run" });

it("draws one Start a run button for the first written plan, however many are written", () => {
  const { store } = renderPage(<RunPage />, PATH, written, {
    holder: "me",
    run: runState({ workspace: "done", active_plan: null, preselected: null }),
  });

  expect(start()).toHaveLength(1);
  fireEvent.click(start()[0]!);
  expect(store.get(runDialogAtom)).toEqual({ open: true, plan: ONE });
});

it("targets the start_run offer's plan, then the preselected one", () => {
  const offered = atlas({
    plan_rows: [planRow("one", "written"), planRow("two", "written")],
    primary: offer("start_run", "Start a run", TWO),
  });
  const first = renderPage(<RunPage />, PATH, offered, { holder: "me" });
  fireEvent.click(start()[0]!);
  expect(first.store.get(runDialogAtom).plan).toBe(TWO);
  first.unmount();

  const preselected = renderPage(<RunPage />, PATH, written, {
    holder: "me",
    run: runState({ workspace: "done", active_plan: null, preselected: TWO }),
  });
  fireEvent.click(start()[0]!);
  expect(preselected.store.get(runDialogAtom).plan).toBe(TWO);
});

it("ignores a preselected plan that is not a written plan of this feature", () => {
  const { store } = renderPage(<RunPage />, PATH, written, {
    holder: "me",
    run: runState({ workspace: "done", active_plan: null, preselected: "doc/plans/PLAN-x.md" }),
  });
  fireEvent.click(start()[0]!);
  expect(store.get(runDialogAtom).plan).toBe(ONE);
});

it("makes the start button the page's primary only when the checklist's primary is start_run", () => {
  const primary = renderPage(
    <RunPage />,
    PATH,
    atlas({ plan_rows: [planRow("one", "written")], primary: offer("start_run", "Start", ONE) }),
  );
  expect(start()[0]!.getAttribute("data-look")).toBe("primary");
  primary.unmount();

  renderPage(<RunPage />, PATH, atlas({ plan_rows: [planRow("one", "written")] }));
  expect(start()[0]!.getAttribute("data-look")).toBe("secondary");
});

it("blocks the start button with one reason while another browser holds authoring", () => {
  renderPage(<RunPage />, PATH, written, { holder: "other" });

  expect(start()[0]!.getAttribute("aria-disabled")).toBe("true");
  expect(screen.getAllByText("Ada holds authoring")).toHaveLength(1);
});

it("draws no start button when no plan is written", () => {
  renderPage(<RunPage />, PATH, atlas({ plan_rows: [planRow("one", "running")] }), {
    holder: "me",
  });
  expect(screen.queryByRole("button", { name: "Start a run" })).toBeNull();
});

it("reads a run of this feature's plan with its merge progress", () => {
  const checklist = atlas({ plan_rows: [planRow("one", "running"), planRow("two", "written")] });
  renderPage(<RunPage />, PATH, checklist, {
    run: runState({ workspace: "running", active_plan: ONE }),
    strip: runStrip({ plan: ONE, merged: 1, total: 3 }),
  });

  expect(screen.getByText("one is running: 1 of 3 stages merged.")).toBeTruthy();
  expect(screen.getByRole("link", { name: "Open dashboard" })).toBeTruthy();
});

it("does not read another feature's run as this feature's", () => {
  renderPage(<RunPage />, PATH, written, {
    holder: "me",
    run: runState({ workspace: "running", active_plan: "doc/plans/other/PLAN-z.md" }),
    strip: runStrip({ plan: "doc/plans/other/PLAN-z.md", merged: 2, total: 4 }),
  });

  expect(screen.getByText("Another plan holds the workspace.")).toBeTruthy();
  expect(screen.queryByText(/stages merged/)).toBeNull();
  expect(screen.queryByRole("link", { name: "Open dashboard" })).toBeNull();
});

it("says nothing is running, and offers no dashboard link without a dashboard url", () => {
  renderPage(<RunPage />, PATH, written, {
    run: runState({ workspace: "absent", active_plan: null, dashboard_url: null }),
  });

  expect(screen.getByText("Nothing is running.")).toBeTruthy();
  expect(screen.queryByRole("link", { name: "Open dashboard" })).toBeNull();
});

it("says the run state could not be read, with Start blocked, when the request fails", async () => {
  serveRunState(() => json({ error: "internal", message: "boom" }, 500));
  renderPage(<RunPage />, PATH, written, { holder: "me" });

  const status = await screen.findByRole("status");
  expect(status.textContent).toBe("Could not read the run state.");
  expect(screen.queryByText("Nothing is running.")).toBeNull();
  expect(start()[0]!.getAttribute("aria-disabled")).toBe("true");
  expect(screen.getAllByText("Run state unavailable.")).toHaveLength(1);
});

it("says it is checking while the run state request is out, with Start enabled", () => {
  vi.stubGlobal("fetch", () => new Promise<Response>(() => {}));
  renderPage(<RunPage />, PATH, written, { holder: "me" });

  expect(screen.getByRole("status").textContent).toBe("Checking for a run.");
  expect(start()[0]!.getAttribute("aria-disabled")).toBeNull();
});

it("leaves the checking line when the first of two overlapping requests answers", async () => {
  const answers: ((response: Response) => void)[] = [];
  vi.stubGlobal(
    "fetch",
    () =>
      new Promise<Response>((resolve) => {
        answers.push(resolve);
      }),
  );
  const { store } = renderPage(<RunPage />, PATH, written, { holder: "me" });
  // The shell's own fetch on arrival goes out after the page's, and overtakes it.
  void fetchRunState(store);
  expect(answers).toHaveLength(2);
  expect(screen.getByRole("status").textContent).toBe("Checking for a run.");

  act(() => answers[0]!(json(runState({ workspace: "done", active_plan: null }))));

  expect((await screen.findByRole("status")).textContent).toBe("Nothing is running.");
  act(() => answers[1]!(json({ error: "internal", message: "boom" }, 500)));
  expect(screen.getByRole("status").textContent).toBe("Nothing is running.");
});

it("follows the run state when the strip's plan changes under an open page", async () => {
  const running = runState({ workspace: "running", active_plan: ONE, offer: "none_running" });
  let served: unknown = running;
  serveRunState(() => json(served));
  const checklist = atlas({ plan_rows: [planRow("one", "done"), planRow("two", "written")] });
  const strip = runStrip({ plan: ONE, merged: 1, total: 3 });
  const { store } = renderPage(<RunPage />, PATH, checklist, { run: running, strip });
  const socket = stripSocket(store);
  expect((await screen.findByRole("status")).textContent).toBe(
    "one is running: 1 of 3 stages merged.",
  );

  // The run finished: the plan is renamed, the strip loses it, and the workspace reads done.
  served = runState({ workspace: "done", active_plan: null, offer: "start", preselected: null });
  act(() => socket.push(runStrip({ plan: null, daemon_running: false, merged: 0, total: 0 })));

  expect(await screen.findByText("Nothing is running.")).toBeTruthy();
  expect(screen.getByRole("status").textContent).toBe("Nothing is running.");
  socket.dispose();
});

it("does not call a plan file left as running a live run", async () => {
  const one = atlas({ plan_rows: [planRow("one", "running"), planRow("two", "written")] });
  const first = renderPage(<RunPage />, PATH, one, {
    run: runState({ workspace: "absent", active_plan: null }),
  });
  expect((await screen.findByRole("status")).textContent).toBe(
    "No run is active here. one still reads as running.",
  );
  expect(screen.queryByText("Nothing is running.")).toBeNull();
  first.unmount();

  const several = atlas({ plan_rows: [planRow("one", "running"), planRow("two", "running")] });
  renderPage(<RunPage />, PATH, several, {
    run: runState({ workspace: "done", active_plan: null }),
  });
  expect((await screen.findByRole("status")).textContent).toBe(
    "No run is active here. 2 plans still read as running.",
  );
});

it("keeps the plain idle line when no plan reads as running", async () => {
  renderPage(<RunPage />, PATH, written, {
    run: runState({ workspace: "done", active_plan: null }),
  });
  expect((await screen.findByRole("status")).textContent).toBe("Nothing is running.");
});

it("names the listed plans that are not written yet under the rows", () => {
  const listed = atlas({
    plan_rows: [planRow("one", "written")],
    plans: { listed: 3, total: 1, written: 1, running: 0, done: 0 },
  });
  renderPage(<RunPage />, PATH, listed, {
    run: runState({ workspace: "absent", active_plan: null }),
  });

  expect(screen.getByText("2 listed plans are not written yet.")).toBeTruthy();
  expect(screen.getByRole("link", { name: "Go to Plans" }).getAttribute("href")).toBe(
    "/f/atlas/engineering",
  );
});

it("refuses a dashboard url that is not http", () => {
  renderPage(<RunPage />, PATH, written, {
    run: runState({ workspace: "absent", active_plan: null, dashboard_url: "javascript:alert(1)" }),
  });
  expect(screen.queryByRole("link", { name: "Open dashboard" })).toBeNull();
});

it("lists every plan with its state, linked to its concept page", () => {
  renderPage(
    <RunPage />,
    PATH,
    atlas({ plan_rows: [planRow("one", "done"), planRow("two", "written")] }),
  );

  const list = screen.getByRole("list", { name: "Plans" });
  const rows = within(list).getAllByRole("listitem");
  expect(rows.map((row) => row.textContent)).toEqual(["onedone", "twowritten"]);
  expect(within(rows[0]!).getByRole("link", { name: "one" }).getAttribute("href")).toBe(
    "/f/atlas/engineering/one",
  );
});

const finished = (secondary: ReturnType<typeof offer>[]) =>
  atlas({
    plan_rows: [planRow("one", "done"), planRow("two", "done")],
    current: { kind: "ready_all_done" },
    secondary,
  });

it("links a built feature's Run page to what was built and draws no follow-up button", () => {
  renderPage(<RunPage />, PATH, finished([offer("plan_follow_ups", "Plan follow-ups")]));

  expect(screen.getByRole("link", { name: "See what was built" }).getAttribute("href")).toBe(
    "/f/atlas/built",
  );
  expect(screen.queryByRole("button", { name: "Plan follow-ups" })).toBeNull();
  expect(screen.queryByText("Next round")).toBeNull();
  expect(handler).not.toHaveBeenCalled();
});

it("draws no built link while a plan is still written", () => {
  renderPage(<RunPage />, PATH, written);

  expect(screen.queryByRole("link", { name: "See what was built" })).toBeNull();
});

it("sends an empty feature to Plans with one sentence", () => {
  renderPage(<RunPage />, PATH, atlas({ plan_rows: [] }));

  expect(screen.getByRole("heading", { level: 1, name: "Run" })).toBeTruthy();
  expect(screen.getByText(/Write a plan first/)).toBeTruthy();
  expect(screen.getByRole("link", { name: "Go to Plans" }).getAttribute("href")).toBe(
    "/f/atlas/engineering",
  );
});
