import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import hello from "@/plan/api/fixtures/hello.json";
import fixture from "@/plan/api/fixtures/run-state.json";
import { runStateSchema, type RunStateView } from "@/plan/api/schema-run";
import type { connectPlanSocket } from "@/plan/api/socket";
import { runDialogAtom } from "@/plan/offers/actions-run";
import {
  authorBlockReasonAtom,
  authoringAtom,
  connectionAtom,
  planSocketAtom,
  serverInfoAtom,
  youAtom,
} from "@/plan/state/atoms";

import { RunDialog } from "./run-dialog";
import { installRunSubscription, runOutputAtom, runStateAtom } from "./use-run";

function holderStore() {
  const store = createStore();
  store.set(serverInfoAtom, { ...hello.server, read_only: [] });
  store.set(youAtom, hello.you);
  store.set(authoringAtom, { holder: { ...hello.you, away_since: null, frees_at: null } });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: 0 });
  initClient(store);
  return store;
}

function renderRun(patch: Partial<RunStateView> = {}, plan: string | null = null) {
  const state = {
    ...runStateSchema.parse(fixture),
    abandon: null,
    doc_changes: [],
    dashboard_url: null,
    ...patch,
  };
  const store = holderStore();
  store.set(runStateAtom, state);
  store.set(runDialogAtom, { open: true, plan });
  const fetchSpy = vi.fn<typeof fetch>(async (input) => {
    const url = String(input);
    return new Response(
      JSON.stringify(
        url.endsWith("/state")
          ? state
          : url.endsWith("/dashboard")
            ? { url: "http://localhost:7373/" }
            : { accepted: true },
      ),
    );
  });
  vi.stubGlobal("fetch", fetchSpy);
  const view = render(
    <Provider store={store}>
      <MemoryRouter>
        <RunDialog />
      </MemoryRouter>
    </Provider>,
  );
  return { store, state, fetchSpy, ...view };
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it.each([
  ["absent", "start", "Start"],
  ["initialized", "start_initialized", "Start the run"],
  ["stopped", "resume", "Resume"],
  ["done", "start_next", "Start the next plan"],
] as const)("%s workspace offers %s", async (workspace, offer, label) => {
  renderRun({ workspace, offer });
  const button = await screen.findByRole("button", { name: label });
  expect(button.getAttribute("aria-disabled")).toBeNull();
});

it("none_running shows the dashboard and no Start button", async () => {
  renderRun({ workspace: "running", offer: "none_running" });
  expect(screen.getByText("The plan is running")).toBeTruthy();
  expect(await screen.findByRole("link", { name: "Open dashboard" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: /^Start/ })).toBeNull();
});

it.each([
  {
    doc_changes: [],
    blocking_changes: [{ path: "src/main.rs", status: " M" }],
    reason: "Commit or stash the blocking changes first",
  },
  {
    doc_changes: [{ path: "doc/plans/PLAN-example.md", status: " M" }],
    blocking_changes: [],
    reason: "Commit the changes under doc/ first",
  },
])("Start describes why changes block execution: $reason", async ({ reason, ...patch }) => {
  const { fetchSpy } = renderRun({ offer: "start", ...patch });
  const button = screen.getByRole("button", { name: "Start", description: reason });
  fireEvent.click(button);
  await waitFor(() => expect(fetchSpy).toHaveBeenCalled());
  expect(fetchSpy.mock.calls.some(([url]) => String(url).endsWith("/start"))).toBe(false);
});

it("prefills the commit message and posts only checked doc paths", async () => {
  const { fetchSpy } = renderRun({
    doc_changes: [
      { path: "doc/plans/PLAN-example.md", status: " M" },
      { path: "doc/notes.md", status: "??" },
    ],
  });
  const message = screen.getByRole<HTMLTextAreaElement | HTMLInputElement>("textbox", {
    name: "Commit message",
  });
  expect(message.value).toBe("docs(plan): planning documents for PLAN-example.md");
  expect(screen.getByRole<HTMLInputElement>("checkbox", { name: /doc\/notes.md/ }).checked).toBe(
    true,
  );
  fireEvent.click(screen.getByRole("checkbox", { name: /doc\/notes.md/ }));
  fireEvent.change(screen.getByRole("textbox", { name: "Commit message" }), {
    target: { value: "docs(plan): ready" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Commit changes under doc/" }));
  await waitFor(() =>
    expect(fetchSpy).toHaveBeenCalledWith(
      "/api/run/commit",
      expect.objectContaining({
        body: JSON.stringify({
          paths: ["doc/plans/PLAN-example.md"],
          message: "docs(plan): ready",
        }),
      }),
    ),
  );
});

it("Start posts the selected plan, backend and clean requirement", async () => {
  const other = "doc/plans/PLAN-other.md";
  const { fetchSpy } = renderRun({
    workspace: "initialized",
    offer: "start_initialized",
    plans: [
      { path: "doc/plans/PLAN-example.md", slug: "example", listed: true },
      { path: other, slug: "other", listed: false },
    ],
  });
  fireEvent.change(screen.getByRole("combobox", { name: "Plan" }), { target: { value: other } });
  fireEvent.click(screen.getByRole("radio", { name: "Native" }));
  expect(screen.getByText(/Starting this plan uses --clean/)).toBeTruthy();
  expect(screen.getByRole("option", { name: /PLAN-other.md \(unlisted\)/ })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Start the run" }));
  await waitFor(() =>
    expect(fetchSpy).toHaveBeenCalledWith(
      "/api/run/start",
      expect.objectContaining({
        body: JSON.stringify({ plan: other, backend: "native", clean: true }),
      }),
    ),
  );
});

it("uses the atom's plan before the server preselection", () => {
  renderRun(
    {
      doc_changes: [{ path: "doc/notes.md", status: " M" }],
      plans: [
        { path: "doc/plans/PLAN-example.md", slug: "example", listed: true },
        { path: "doc/plans/PLAN-other.md", slug: "other", listed: false },
      ],
    },
    "doc/plans/PLAN-other.md",
  );
  expect(screen.getByRole<HTMLSelectElement>("combobox", { name: "Plan" }).value).toBe(
    "doc/plans/PLAN-other.md",
  );
  expect(
    screen.getByRole<HTMLTextAreaElement | HTMLInputElement>("textbox", { name: "Commit message" })
      .value,
  ).toBe("docs(plan): planning documents for PLAN-other.md");
});

it("falls back to the state's preselected plan", () => {
  renderRun();
  expect(screen.getByRole<HTMLSelectElement>("combobox", { name: "Plan" }).value).toBe(
    "doc/plans/PLAN-example.md",
  );
});

it("hides Native when the server does not offer it", () => {
  renderRun({ backends: ["tmux"] });
  expect(screen.queryByRole("radio", { name: "Native" })).toBeNull();
  expect(screen.getByRole<HTMLInputElement>("radio", { name: "tmux" }).checked).toBe(true);
});

it("shows the clean notice supplied by the server", () => {
  renderRun({ clean_notice: true });
  expect(screen.getByText(/Starting this plan uses --clean/)).toBeTruthy();
});

it("renders Abandon beside Resume when abandonment is available", () => {
  renderRun({ abandon: runStateSchema.parse(fixture).abandon });
  expect(screen.getByRole("button", { name: "Resume" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Abandon run" })).toBeTruthy();
});

it("delegates the finish-abandoning control to AbandonDialog", () => {
  renderRun({ finish_abandon: "doc/plans/IN_PROGRESS-PLAN-example.md" });
  expect(screen.getByText("This plan's run was removed but the plan was not retired")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Finish abandoning" })).toBeTruthy();
});

it("Start and Commit describe the authoring restriction first", () => {
  const { store } = renderRun({
    offer: "start",
    doc_changes: [{ path: "doc/notes.md", status: " M" }],
  });
  act(() =>
    store.set(authoringAtom, {
      holder: {
        ...hello.you,
        public_id: "someone-else",
        name: "Other writer",
        away_since: null,
        frees_at: null,
      },
    }),
  );
  const reason = store.get(authorBlockReasonAtom);
  expect(screen.getByRole("button", { name: "Start", description: reason! })).toBeTruthy();
  expect(
    screen.getByRole("button", { name: "Commit changes under doc/", description: reason! }),
  ).toBeTruthy();
});

it("Start and Commit need the holder even while nobody holds authoring", async () => {
  const reason = "Start a chat to take authoring, then commit or start a run.";
  const { store, fetchSpy } = renderRun({
    offer: "start",
    doc_changes: [{ path: "doc/notes.md", status: " M" }],
  });
  act(() => store.set(authoringAtom, { holder: null }));
  expect(store.get(authorBlockReasonAtom)).toBeNull();
  const start = screen.getByRole("button", { name: "Start", description: reason });
  const commit = screen.getByRole("button", {
    name: "Commit changes under doc/",
    description: reason,
  });
  fireEvent.click(commit);
  fireEvent.click(start);
  await act(async () => {});
  expect(fetchSpy.mock.calls.some(([url]) => /\/api\/run\/(start|commit)$/.test(String(url)))).toBe(
    false,
  );
});

it("sends one request when Start is clicked twice before the form re-renders", async () => {
  const { fetchSpy } = renderRun({ offer: "start", doc_changes: [] });
  const start = screen.getByRole("button", { name: "Start" });
  act(() => {
    start.click();
    start.click();
  });
  const posts = () => fetchSpy.mock.calls.filter(([url]) => String(url).endsWith("/api/run/start"));
  await waitFor(() => expect(posts()).toHaveLength(1));
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 20));
  });
  expect(posts()).toHaveLength(1);
});

it("shows format characters in paths and branch names as visible markers", () => {
  const evil = "doc/\u202Etxt.md";
  renderRun({
    offer: "start",
    branch: "main\u202E",
    doc_changes: [{ path: evil, status: " M" }],
    blocking_changes: [{ path: "src/\u200Bhidden.rs", status: "??" }],
    plans: [{ path: "doc/plans/PLAN-\u202Ex.md", slug: "x", listed: false }],
    preselected: "doc/plans/PLAN-\u202Ex.md",
  });
  expect(screen.getByText("doc/⟨U+202E⟩txt.md ( M)")).toBeTruthy();
  expect(screen.getByText("src/⟨U+200B⟩hidden.rs (??)")).toBeTruthy();
  expect(screen.getByText("merges into main⟨U+202E⟩")).toBeTruthy();
  expect(screen.getByRole("option", { name: /PLAN-⟨U\+202E⟩x\.md/ })).toBeTruthy();
  expect(document.body.textContent).not.toMatch(/[\u202E\u200B]/);
});

it("blocks Start while a job is busy and renders its output as text", () => {
  const { store } = renderRun({ offer: "start", busy: "commit" });
  act(() =>
    store.set(runOutputAtom, {
      ...store.get(runOutputAtom),
      commit: ["<img src=x onerror=alert(1)>"],
    }),
  );
  expect(
    screen.getByRole("button", { name: "Start", description: "Waiting for commit to finish" }),
  ).toBeTruthy();
  expect(screen.getByLabelText("commit output").textContent).toContain(
    "<img src=x onerror=alert(1)>",
  );
  expect(document.querySelector("img")).toBeNull();
});

it("shows request failures without starting a run", async () => {
  const { fetchSpy } = renderRun({ doc_changes: [{ path: "doc/notes.md", status: " M" }] });
  fetchSpy.mockImplementation(
    async () =>
      new Response(JSON.stringify({ error: "refused", message: "Signing failed" }), {
        status: 409,
      }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Commit changes under doc/" }));
  expect(await screen.findByText("Signing failed")).toBeTruthy();
  expect(fetchSpy.mock.calls.some(([url]) => String(url).endsWith("/start"))).toBe(false);
});

it("announces a failed action as an alert", async () => {
  const { fetchSpy } = renderRun({ doc_changes: [{ path: "doc/notes.md", status: " M" }] });
  fetchSpy.mockImplementation(
    async () =>
      new Response(JSON.stringify({ error: "refused", message: "Signing failed" }), {
        status: 409,
      }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Commit changes under doc/" }));
  expect((await screen.findByRole("alert")).textContent).toBe("Signing failed");
});

it("refreshes after a resync missed a run_state frame", async () => {
  const { store, state, fetchSpy } = renderRun({ workspace: "absent", offer: "start" });
  await act(async () => {});
  let resync = () => {};
  const socket: ReturnType<typeof connectPlanSocket> = {
    send: () => true,
    dispose: () => {},
    subscribe: () => () => {},
    onResync: (handler) => {
      resync = handler;
      return () => {
        resync = () => {};
      };
    },
  };
  act(() => store.set(planSocketAtom, socket));
  const dispose = installRunSubscription(store);
  fetchSpy.mockImplementation(
    async (url) =>
      new Response(
        JSON.stringify(
          String(url).endsWith("/state")
            ? { ...state, workspace: "running", offer: "none_running" }
            : { url: "http://localhost:7373/" },
        ),
      ),
  );
  act(() => resync());
  expect(await screen.findByText("The plan is running")).toBeTruthy();
  dispose();
});

it("renders nothing when the dialog atom is closed", () => {
  const { store } = renderRun();
  act(() => store.set(runDialogAtom, { open: false, plan: null }));
  expect(screen.queryByRole("dialog")).toBeNull();
});
