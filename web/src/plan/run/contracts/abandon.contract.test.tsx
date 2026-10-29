import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import hello from "@/plan/api/fixtures/hello.json";
import runStateFixture from "@/plan/api/fixtures/run-state.json";
import { runStateSchema } from "@/plan/api/schema-run";
import { authoringAtom, connectionAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";

import * as m from "../abandon-dialog";

// The viewer holds authoring and the server is writable: Abandon is holder-only.
function holderStore() {
  const store = createStore();
  store.set(serverInfoAtom, { ...hello.server, read_only: [] });
  store.set(youAtom, hello.you);
  store.set(authoringAtom, { holder: { ...hello.you, away_since: null, frees_at: null } });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: Date.now() });
  initClient(store);
  return store;
}

function jsonReply(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}

function abandonPosts(spy: ReturnType<typeof vi.fn<typeof fetch>>): unknown[] {
  return spy.mock.calls.filter(([input, init]) => {
    const url = input instanceof Request ? input.url : String(input);
    const method = init?.method ?? (input instanceof Request ? input.method : "GET");
    return url.includes("/api/run/abandon") && method.toUpperCase() === "POST";
  });
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

// One top-level it() carrying the whole contract name: vitest 5 joins describe and it with
// " > ", so loom's space-joined -t filter would select nothing under a describe block.
it("abandon dialog declining the unmerged confirmation sends no request", async () => {
  const state = runStateSchema.parse(runStateFixture);
  expect(state.abandon?.unmerged_branches).toHaveLength(1);
  const spy = vi.fn<typeof fetch>(async () => jsonReply(state));
  vi.stubGlobal("fetch", spy);

  render(
    <Provider store={holderStore()}>
      <MemoryRouter>
        <m.AbandonDialog state={state} />
      </MemoryRouter>
    </Provider>,
  );

  // The trigger and the first confirmation may both read "Abandon run": click through
  // until the second confirmation, the one naming the unmerged branch, is open.
  const secondConfirm = /^delete 1 branch/i;
  for (let step = 0; step < 2 && !screen.queryByRole("button", { name: secondConfirm }); step++) {
    const buttons = await screen.findAllByRole("button", { name: /^abandon run/i });
    fireEvent.click(buttons[buttons.length - 1]!);
  }
  const deleteButton = await screen.findByRole("button", { name: secondConfirm });
  const second = deleteButton.closest<HTMLElement>('[role="dialog"], [role="alertdialog"]');
  expect(second).not.toBeNull();
  fireEvent.click(within(second!).getByRole("button", { name: /^cancel$/i }));

  await waitFor(() => expect(screen.queryByRole("button", { name: secondConfirm })).toBeNull());
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 20));
  });
  expect(abandonPosts(spy)).toEqual([]);
});
