import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import hello from "@/plan/api/fixtures/hello.json";
import optionsFixture from "@/plan/api/fixtures/pressure-options.json";
import runFixture from "@/plan/api/fixtures/pressure-run.json";
import { pressureOptionsSchema, type PressureOptions } from "@/plan/api/schema-pressure";
import { pressureDialogAtom } from "@/plan/offers/actions-pressure";
import { authoringAtom, connectionAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";

import { PressureDialog } from "./pressure-dialog";
import { pressureOptionsAtom } from "./use-pressure";

const options = pressureOptionsSchema.parse(optionsFixture);
const plan = "doc/plans/PLAN-example.md";

function holderStore(value: PressureOptions = options) {
  const store = createStore();
  store.set(serverInfoAtom, { ...hello.server, read_only: [] });
  store.set(youAtom, hello.you);
  store.set(authoringAtom, { holder: { ...hello.you, away_since: null, frees_at: null } });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: 0 });
  store.set(pressureOptionsAtom, { ok: true, value });
  store.set(pressureDialogAtom, { open: true, plan });
  initClient(store);
  return store;
}

// The form mounts only after the open-time options refetch settles, so wait for that.
async function mount(store = holderStore()) {
  render(
    <Provider store={store}>
      <MemoryRouter>
        <PressureDialog />
      </MemoryRouter>
    </Provider>,
  );
  await waitFor(() =>
    expect(screen.queryByRole("status", { name: "Loading pressure options" })).toBeNull(),
  );
  return store;
}

function jsonReply(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

// Opening the dialog refetches the options, so every test needs a fetch that answers them.
function stubFetch(replies: { options?: () => Response; start?: () => Response } = {}) {
  const spy = vi.fn<typeof fetch>(async (input) =>
    String(input).includes("/api/pressure/options")
      ? (replies.options?.() ?? jsonReply(optionsFixture))
      : (replies.start?.() ?? jsonReply(runFixture)),
  );
  vi.stubGlobal("fetch", spy);
  return spy;
}

beforeEach(() => {
  stubFetch();
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("shows control characters in a plan path and the options reason as visible markers", async () => {
  const hostile = "doc/plans/PLAN-‮evil.md";
  const unavailable = {
    ...options,
    plans: [hostile],
    available: false,
    disabled_reason: "Codex changed session configuration: .claude/‮x",
  };
  stubFetch({ options: () => jsonReply(unavailable) });
  await mount(holderStore(unavailable));

  const select = screen.getByRole("combobox", { name: "Change plan" }) as HTMLSelectElement;
  expect(within(select).getAllByRole("option")[0]!.textContent).toBe(
    "doc/plans/PLAN-⟨U+202E⟩evil.md",
  );
  expect(select.value).toBe(hostile);
  expect(screen.getByText("Codex changed session configuration: .claude/⟨U+202E⟩x")).toBeTruthy();
});

it("shows control characters in a start refusal as visible markers", async () => {
  stubFetch({
    start: () =>
      jsonReply({ error: "config_blocked", message: "Starts are blocked: AGENTS.md‮.md" }, 409),
  });
  await mount();

  fireEvent.click(screen.getByRole("button", { name: "Start pressure test" }));

  expect((await screen.findByRole("alert")).textContent).toBe(
    "Starts are blocked: AGENTS.md⟨U+202E⟩.md",
  );
});
