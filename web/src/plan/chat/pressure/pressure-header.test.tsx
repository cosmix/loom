import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import chatFixture from "@/plan/api/fixtures/chat-snapshot.json";
import hello from "@/plan/api/fixtures/hello.json";
import runFixture from "@/plan/api/fixtures/pressure-run.json";
import { chatSnapshotSchema, sessionViewSchema } from "@/plan/api/schema-chat";
import {
  pressureRunSchema,
  type PressureEnd,
  type PressureRunView,
} from "@/plan/api/schema-pressure";
import { serverInfoSchema } from "@/plan/api/schema";
import { authoringAtom, connectionAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";
import { snapshotAtom } from "../model";
import { PressureHeader } from "./pressure-header";
import { pressureRunAtom, pressureRunWaitingAtom } from "./use-pressure";

const session = sessionViewSchema.parse({
  ...chatFixture.session,
  id: "session-1",
  kind: "pressure",
});
const confirmText =
  "Stop the pressure test? The plan keeps the edits made so far, and no record is written.";

function setupStore(
  run: PressureRunView | null = pressureRunSchema.parse(runFixture),
  holder = true,
) {
  const store = createStore();
  store.set(serverInfoAtom, { ...hello.server, read_only: [] });
  store.set(youAtom, holder ? hello.you : { public_id: "viewer", name: "Viewer" });
  store.set(authoringAtom, { holder: { ...hello.you, away_since: null, frees_at: null } });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: 4000 });
  store.set(pressureRunAtom, run);
  initClient(store);
  return store;
}

function mount(store = setupStore()) {
  return render(
    <Provider store={store}>
      <MemoryRouter>
        <PressureHeader session={session} />
      </MemoryRouter>
    </Provider>,
  );
}

function reply(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function endedRun(end: Partial<PressureEnd>): PressureRunView {
  return {
    ...pressureRunSchema.parse(runFixture),
    ended: { kind: "interrupted", by: null, step: null, reason: null, line: null, ...end },
  };
}

beforeEach(() => {
  vi.spyOn(Date, "now").mockReturnValue(4000);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("shows round 2 of 2 at the Codex step with elapsed time and total tokens", () => {
  mount();

  expect(screen.getByText("Pressure test")).toBeTruthy();
  expect(screen.getByText("round 2 of 2")).toBeTruthy();
  const head = within(document.querySelector<HTMLElement>(".chat-run-header")!);
  expect(head.getByText("PLAN-example.md").parentElement?.classList.contains("font-mono")).toBe(
    true,
  );
  expect(screen.getByText("3s")).toBeTruthy();
  expect(head.getByText("175 tokens")).toBeTruthy();
  expect(head.queryByText("·")).toBeNull();
  expect(screen.getByText(/gpt-6.1-terra/)).toBeTruthy();
  expect(screen.queryByRole("button", { name: "End chat" })).toBeNull();
});

it("renders nothing without a run", () => {
  const { container } = mount(setupStore(null));
  expect(container.innerHTML).toBe("");
});

it("shows the waiting note until the run's next update", () => {
  const store = setupStore();
  store.set(pressureRunWaitingAtom, true);
  mount(store);
  expect(screen.getByText("reconnected; waiting for the run's next update")).toBeTruthy();

  act(() => store.set(pressureRunWaitingAtom, false));
  expect(screen.queryByText("reconnected; waiting for the run's next update")).toBeNull();
});

it("confirms Stop and posts to the whole-run endpoint", async () => {
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
  const fetch = vi.fn<typeof globalThis.fetch>(async () => reply({ stopping: true }));
  vi.stubGlobal("fetch", fetch);
  mount();

  fireEvent.click(screen.getByRole("button", { name: "Stop" }));

  expect(confirm).toHaveBeenCalledWith(confirmText);
  await waitFor(() =>
    expect(fetch).toHaveBeenCalledWith(
      "/api/pressure/stop",
      expect.objectContaining({ method: "POST" }),
    ),
  );
});

it("draws Stop as the project's secondary button, the tone on its glyph only", () => {
  mount();
  const stop = screen.getByRole("button", { name: "Stop" });
  expect(stop.getAttribute("data-look")).toBe("secondary");
  expect(stop.closest(".ml-auto")?.className).toBe("ml-auto");
  expect(stop.querySelector("svg")?.getAttribute("class")).toContain("text-(--tone-blocked)");
});

it("sends no request when Stop is declined", () => {
  vi.spyOn(window, "confirm").mockReturnValue(false);
  const fetch = vi.fn<typeof globalThis.fetch>();
  vi.stubGlobal("fetch", fetch);
  mount();

  fireEvent.click(screen.getByRole("button", { name: "Stop" }));

  expect(fetch).not.toHaveBeenCalled();
});

it("shows a Stop error as text", async () => {
  const message = "<img src=x onerror=alert(1)>";
  vi.spyOn(window, "confirm").mockReturnValue(true);
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => reply({ error: "busy", message }, 409)),
  );
  mount();

  fireEvent.click(screen.getByRole("button", { name: "Stop" }));

  expect((await screen.findByRole("alert")).textContent).toContain(message);
  expect(document.querySelector("img")).toBeNull();
});

it("hides Stop from a browser that does not hold authoring", () => {
  mount(setupStore(pressureRunSchema.parse(runFixture), false));
  expect(screen.queryByRole("button", { name: "Stop" })).toBeNull();
});

it("hides Stop while the server is read-only even if this browser holds authoring", () => {
  const store = setupStore();
  store.set(serverInfoAtom, serverInfoSchema.parse(hello.server));
  mount(store);
  expect(screen.queryByRole("button", { name: "Stop" })).toBeNull();
});

it.each<[Partial<PressureEnd>, string]>([
  [{ kind: "recorded", line: "rounds complete" }, "Pressure test recorded · rounds complete"],
  [{ kind: "recorded" }, "Pressure test recorded"],
  [{ kind: "stopped", by: "Ada" }, "Stopped by Ada"],
  [
    { kind: "failed", step: "codex", reason: "process exited" },
    "Step codex failed: process exited",
  ],
  [{ kind: "interrupted" }, "Interrupted"],
])("renders the end line for %j", (end, expected) => {
  mount(setupStore(endedRun(end)));
  expect(screen.getByText(expected)).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Stop" })).toBeNull();
});

it("lists configuration changes and leaves the confirmation to the config-block notice", () => {
  const reason = "Codex changed session configuration: AGENTS.md, .codex/config.toml";
  const fetch = vi.fn<typeof globalThis.fetch>();
  vi.stubGlobal("fetch", fetch);
  mount(setupStore(endedRun({ kind: "failed", step: "codex", reason })));
  expect(screen.getByText(reason)).toBeTruthy();

  expect(screen.queryByRole("button", { name: "I have checked these files" })).toBeNull();
  expect(fetch).not.toHaveBeenCalled();
});

it("shows configuration changes to a read-only browser without a confirmation control", () => {
  const reason = "Codex changed session configuration: AGENTS.md";
  mount(setupStore(endedRun({ kind: "failed", step: "codex", reason }), false));
  expect(screen.getByText(reason)).toBeTruthy();
  expect(screen.queryByRole("button", { name: "I have checked these files" })).toBeNull();
});

it("renders markup in the plan name and stopping author as text", () => {
  const run = endedRun({ kind: "stopped", by: "<b>Ada</b>" });
  run.plan = "doc/plans/<img src=x onerror=alert(1)>.md";
  mount(setupStore(run));
  expect(screen.getByText("<img src=x onerror=alert(1)>.md")).toBeTruthy();
  expect(screen.getByText("Stopped by <b>Ada</b>")).toBeTruthy();
  expect(document.querySelector("img, b")).toBeNull();
});

it("renders markup in a configuration failure reason as text", () => {
  const reason = "Codex changed session configuration: <img src=x onerror=alert(1)>";
  mount(setupStore(endedRun({ kind: "failed", step: "codex", reason })));
  expect(screen.getByText(reason)).toBeTruthy();
  expect(document.querySelector("img")).toBeNull();
});

it("lists each step's edits and tokens with file links when no turn is known", () => {
  mount();
  const footer = within(screen.getByLabelText("Round 2 /pressure"));
  expect(footer.getByText("175 tokens")).toBeTruthy();
  expect(footer.getByRole("link", { name: "PLAN-example.md" }).getAttribute("href")).toBe(
    "/files/plans/PLAN-example.md",
  );
  expect(footer.queryByRole("link", { name: "Diff" })).toBeNull();
});

it.each(["current", "previous"])(
  "links edits to a known completed turn in the %s session",
  (where) => {
    const store = setupStore();
    const snapshot = chatSnapshotSchema.parse(chatFixture);
    const turn = {
      ...snapshot.turns[0]!,
      changed_files: [{ path: "doc/plans/PLAN-example.md", before_hash: "before", by_tool: true }],
    };
    store.set(
      snapshotAtom,
      where === "current"
        ? { ...snapshot, session, turns: [turn] }
        : { ...snapshot, previous: { session, turns: [turn] } },
    );
    mount(store);
    expect(screen.getByRole("link", { name: "Diff" }).getAttribute("href")).toBe(
      "/files/plans/PLAN-example.md?diff=turn&turn=session-1%2Ft1",
    );
  },
);

it("ticks elapsed time each second and freezes it when the run ends", () => {
  vi.restoreAllMocks();
  vi.useFakeTimers();
  vi.setSystemTime(4000);
  const store = setupStore();
  mount(store);
  expect(screen.getByText("3s")).toBeTruthy();

  act(() => vi.advanceTimersByTime(1000));
  expect(screen.getByText("4s")).toBeTruthy();
  act(() =>
    store.set(pressureRunAtom, {
      ...endedRun({ kind: "recorded" }),
      steps: [],
    }),
  );
  act(() => vi.advanceTimersByTime(2000));
  // The done head freezes at the arrival time, in the tag that replaced the live clock.
  expect(screen.getAllByText("4s")).toHaveLength(1);
});

it("shows one head, the done head, above the summary and the timeline once recorded", () => {
  mount(setupStore(endedRun({ kind: "recorded" })));

  expect(document.querySelectorAll(".chat-run-header")).toHaveLength(1);
  expect(screen.getByText("Pressure test finished")).toBeTruthy();
  expect(screen.queryByText("Pressure test")).toBeNull();
  expect(screen.queryByText(/^round \d+ of \d+$/)).toBeNull();
  expect(screen.getAllByRole("region", { name: "Pressure test summary" })).toHaveLength(1);
  const head = document.querySelector(".chat-run-header")!;
  const region = screen.getByRole("region", { name: "Pressure test summary" });
  const timeline = document.querySelector("[data-pressure-timeline]")!;
  expect(head.compareDocumentPosition(region) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(region.compareDocumentPosition(timeline) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(region.querySelector(":scope > div:last-child > a")?.textContent).toBe(
    "Review the changes",
  );
});

it("ignores a second Stop click while the stop request is pending and after it was accepted", async () => {
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
  let answer: (response: Response) => void = () => {};
  const fetch = vi.fn<typeof globalThis.fetch>(
    () =>
      new Promise<Response>((resolve) => {
        answer = resolve;
      }),
  );
  vi.stubGlobal("fetch", fetch);
  mount();
  const stop = screen.getByRole("button", { name: "Stop" });

  fireEvent.click(stop);
  fireEvent.click(stop);

  await waitFor(() => expect(fetch).toHaveBeenCalledTimes(1));
  expect(confirm).toHaveBeenCalledTimes(1);
  expect(screen.getByRole("button", { name: "Stop", description: "Stopping…" })).toBe(stop);
  await act(async () => answer(reply({ stopping: true })));
  fireEvent.click(stop);
  expect(confirm).toHaveBeenCalledTimes(1);
  expect(fetch).toHaveBeenCalledTimes(1);
});

it("enables Stop again after the server refuses the stop", async () => {
  vi.spyOn(window, "confirm").mockReturnValue(true);
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => reply({ error: "no_run", message: "No pressure test is running." }, 409)),
  );
  mount();

  fireEvent.click(screen.getByRole("button", { name: "Stop" }));

  expect((await screen.findByRole("alert")).textContent).toBe("No pressure test is running.");
  expect(screen.getByRole("button", { name: "Stop" }).getAttribute("aria-disabled")).toBeNull();
});

it("shows control characters in edit paths, the end reason and the plan name as markers", () => {
  const reason = "Codex changed session configuration: AGENTS.md\u202E.md";
  const shown = "Codex changed session configuration: AGENTS.md⟨U+202E⟩.md";
  const run = endedRun({ kind: "failed", step: "codex", reason });
  run.plan = "doc/plans/PLAN-\u202Eevil.md";
  run.steps = run.steps.map((step, index) =>
    index === 0 ? { ...step, edits: ["doc/plans/PLAN-\u202Eevil.md"] } : step,
  );
  mount(setupStore(run));

  expect(screen.getByRole("link", { name: "PLAN-⟨U+202E⟩evil.md" })).toBeTruthy();
  expect(screen.getByText(`Step codex failed: ${shown}`)).toBeTruthy();
  expect(screen.getByText(shown)).toBeTruthy();
  // The head's file line and the edit's chip both name the file.
  expect(screen.getAllByText("PLAN-⟨U+202E⟩evil.md")).toHaveLength(2);
  expect(document.body.textContent).not.toContain("\u202E");
});

it("labels each step footer as a group", () => {
  mount();
  expect(screen.getByRole("group", { name: "Round 2 /pressure" })).toBeTruthy();
});
