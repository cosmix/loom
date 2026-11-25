import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import snapshot from "@/api/fixtures/snapshot.json";
import { stageSummarySchema } from "@/api/schema";
import { terminalGate } from "@/components/terminal/terminal-glyph";
import { initClient } from "@/plan/api/client";
import hello from "@/plan/api/fixtures/hello.json";
import runStateFixture from "@/plan/api/fixtures/run-state.json";
import { runStateSchema } from "@/plan/api/schema-run";
import { runDialogAtom } from "@/plan/offers/actions-run";
import { RunDialog } from "@/plan/run/run-dialog";
import { runStateAtom } from "@/plan/run/use-run";
import { authoringAtom, connectionAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function stageWithoutBackend(sessionAlive: boolean) {
  const source = snapshot.status.stages.find((stage) => stage.id === "server");
  if (!source) throw new Error("fixture stage server is missing");
  const raw: Record<string, unknown> = structuredClone(source);
  delete raw.session_backend;
  raw.session_alive = sessionAlive;
  return stageSummarySchema.parse(raw);
}

it("tmux only terminal gate opens a live stage without a backend field", () => {
  expect(terminalGate(true, stageWithoutBackend(true))).toBeNull();
  expect(terminalGate(true, stageWithoutBackend(false))).toBe("No live session");
});

function runStateWithoutBackends() {
  const raw: Record<string, unknown> = structuredClone(runStateFixture);
  delete raw.backends;
  delete raw.default_backend;
  return {
    ...runStateSchema.parse(raw),
    workspace: "absent" as const,
    offer: "start" as const,
    doc_changes: [],
    blocking_changes: [],
    abandon: null,
    dashboard_url: null,
  };
}

function holderStore() {
  const store = createStore();
  store.set(serverInfoAtom, { ...hello.server, read_only: [] });
  store.set(youAtom, hello.you);
  store.set(authoringAtom, { holder: { ...hello.you, away_since: null, frees_at: null } });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: 0 });
  initClient(store);
  return store;
}

it("tmux only run dialog starts a run with plan and clean alone", async () => {
  const state = runStateWithoutBackends();
  const store = holderStore();
  store.set(runStateAtom, state);
  store.set(runDialogAtom, { open: true, plan: null });
  const fetchSpy = vi.fn<typeof fetch>(
    async (input) =>
      new Response(JSON.stringify(String(input).endsWith("/state") ? state : { accepted: true })),
  );
  vi.stubGlobal("fetch", fetchSpy);
  render(
    <Provider store={store}>
      <MemoryRouter>
        <RunDialog />
      </MemoryRouter>
    </Provider>,
  );
  expect(screen.queryByRole("radio", { name: "Native" })).toBeNull();
  expect(screen.queryByRole("radio", { name: "tmux" })).toBeNull();
  expect(screen.queryByText(/^(Native|tmux)$/)).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Start" }));
  await waitFor(() =>
    expect(fetchSpy.mock.calls.some(([url]) => String(url) === "/api/run/start")).toBe(true),
  );
  const call = fetchSpy.mock.calls.find(([url]) => String(url) === "/api/run/start");
  expect(JSON.parse(String(call?.[1]?.body))).toEqual({ plan: state.preselected, clean: false });
});
