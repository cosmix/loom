import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { afterEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import fixture from "@/plan/api/fixtures/checklist.json";
import hello from "@/plan/api/fixtures/hello.json";
import { checklistViewSchema, type OfferView } from "@/plan/api/schema";
import { registerOfferAction } from "@/plan/offers/actions";
import { pressureDialogAtom } from "@/plan/offers/actions-pressure";
import { runDialogAtom } from "@/plan/offers/actions-run";
import optionsFixture from "@/plan/api/fixtures/pressure-options.json";
import { pressureOptionsSchema } from "@/plan/api/schema-pressure";
import {
  installPressureSubscription,
  pressureOptionsAtom,
} from "@/plan/chat/pressure/use-pressure";
import { authoringAtom, checklistAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";
import { PlanRunActions } from "./plan-run-actions";

const path = "doc/plans/PLAN-example.md";
const blocked = {
  reason: "Codex changed session configuration: AGENTS.md",
  changed: ["AGENTS.md"],
};
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function holdAuthoring(store: ReturnType<typeof createStore>) {
  store.set(serverInfoAtom, { ...hello.server, read_only: [] });
  store.set(youAtom, hello.you);
  store.set(authoringAtom, { holder: { ...hello.you, away_since: null, frees_at: null } });
}

function optionsWith(overrides: Record<string, unknown>) {
  return pressureOptionsSchema.parse({ ...optionsFixture, config_block: null, ...overrides });
}

it("opens the dialogs for a written plan and respects authoring restrictions", () => {
  const store = createStore();
  render(
    <Provider store={store}>
      <PlanRunActions path={path} state="written" />
    </Provider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Pressure-test" }));
  expect(store.get(pressureDialogAtom)).toEqual({ open: true, plan: path });
  act(() => holdAuthoring(store));
  fireEvent.click(screen.getByRole("button", { name: "Start a run" }));
  expect(store.get(runDialogAtom)).toEqual({ open: true, plan: path });
  act(() =>
    store.set(authoringAtom, {
      holder: { public_id: "other", name: "Ada", away_since: null, frees_at: null },
    }),
  );
  expect(
    screen.getByRole("button", { name: "Pressure-test", description: "Ada holds authoring" }),
  ).toBeTruthy();
  expect(
    screen.getByRole("button", { name: "Start a run", description: "Ada holds authoring" }),
  ).toBeTruthy();
});

it("disables Pressure-test with the server reason when the options say unavailable", () => {
  const store = createStore();
  store.set(pressureOptionsAtom, {
    ok: true,
    value: pressureOptionsSchema.parse({
      ...optionsFixture,
      available: false,
      disabled_reason: "Codex is not installed",
    }),
  });
  render(
    <Provider store={store}>
      <PlanRunActions path={path} state="written" />
    </Provider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Pressure-test" }));
  expect(
    screen.getByRole("button", { name: "Pressure-test", description: "Codex is not installed" }),
  ).toBeTruthy();
  expect(store.get(pressureDialogAtom).open).toBe(false);
  act(() => holdAuthoring(store));
  fireEvent.click(screen.getByRole("button", { name: "Start a run" }));
  expect(store.get(runDialogAtom).open).toBe(true);
});

it("disables Start a run for a browser that does not hold authoring, since the run routes need the holder", () => {
  const store = createStore();
  store.set(serverInfoAtom, { ...hello.server, read_only: [] });
  store.set(youAtom, hello.you);
  store.set(authoringAtom, { holder: null });
  render(
    <Provider store={store}>
      <PlanRunActions path={path} state="written" />
    </Provider>,
  );

  const start = screen.getByRole("button", {
    name: "Start a run",
    description: "Start a chat to take authoring, then start a run.",
  });
  fireEvent.click(start);

  expect(store.get(runDialogAtom).open).toBe(false);
  // Pressure start takes free authoring itself, so it stays available.
  fireEvent.click(screen.getByRole("button", { name: "Pressure-test" }));
  expect(store.get(pressureDialogAtom).open).toBe(true);
});

it("falls back to a generic reason when the server is unavailable without giving one", () => {
  const store = createStore();
  store.set(pressureOptionsAtom, {
    ok: true,
    value: optionsWith({ available: false, disabled_reason: null }),
  });
  render(
    <Provider store={store}>
      <PlanRunActions path={path} state="written" />
    </Provider>,
  );

  expect(
    screen.getByRole("button", {
      name: "Pressure-test",
      description: "Pressure-testing is unavailable",
    }),
  ).toBeTruthy();
});

it("shows control characters in the options reason as visible markers", () => {
  const store = createStore();
  store.set(pressureOptionsAtom, {
    ok: true,
    value: optionsWith({ available: false, disabled_reason: "Codex changed: .claude/\u202Ex" }),
  });
  render(
    <Provider store={store}>
      <PlanRunActions path={path} state="written" />
    </Provider>,
  );

  expect(
    screen.getByRole("button", {
      name: "Pressure-test",
      description: "Codex changed: .claude/⟨U+202E⟩x",
    }),
  ).toBeTruthy();
});

it("re-enables Pressure-test once the holder who blocked it releases authoring", async () => {
  const store = createStore();
  store.set(serverInfoAtom, { ...hello.server, read_only: [] });
  store.set(youAtom, hello.you);
  store.set(authoringAtom, {
    holder: { public_id: "other", name: "Ada", away_since: null, frees_at: null },
  });
  store.set(pressureOptionsAtom, {
    ok: true,
    value: optionsWith({ available: false, disabled_reason: "Another browser holds authoring." }),
  });
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => Response.json(optionsWith({}))),
  );
  const dispose = installPressureSubscription(store);
  try {
    render(
      <Provider store={store}>
        <PlanRunActions path={path} state="written" />
      </Provider>,
    );

    act(() => store.set(authoringAtom, { holder: null }));

    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Pressure-test" }).getAttribute("aria-disabled"),
      ).toBeNull(),
    );
    expect(screen.queryByText("Another browser holds authoring.")).toBeNull();
  } finally {
    dispose();
  }
});

it("shows the config block beside the actions and re-enables Pressure-test after the holder confirms", async () => {
  const store = createStore();
  holdAuthoring(store);
  initClient(store);
  store.set(pressureOptionsAtom, {
    ok: true,
    value: optionsWith({
      available: false,
      disabled_reason: blocked.reason,
      config_block: blocked,
    }),
  });
  const fetch = vi.fn<typeof globalThis.fetch>(async (input) =>
    String(input).includes("confirm-config")
      ? Response.json({ cleared: true })
      : Response.json(optionsWith({})),
  );
  vi.stubGlobal("fetch", fetch);
  render(
    <Provider store={store}>
      <PlanRunActions path={path} state="written" />
    </Provider>,
  );
  expect(
    screen.getByRole("button", { name: "Pressure-test", description: blocked.reason }),
  ).toBeTruthy();
  expect(screen.getByRole("region", { name: "Codex configuration change (plan)" })).toBeTruthy();

  fireEvent.click(screen.getByRole("button", { name: "I have checked these files" }));

  await waitFor(() => expect(screen.queryByRole("region")).toBeNull());
  expect(
    screen.getByRole("button", { name: "Pressure-test" }).getAttribute("aria-disabled"),
  ).toBeNull();
  expect(fetch).toHaveBeenCalledWith(
    "/api/pressure/confirm-config",
    expect.objectContaining({ method: "POST" }),
  );
});

// The fixture checklist is the demo feature's; a null-target offer is that feature's own.
const featurePath = "doc/plans/demo/PLAN-example.md";
const review: OfferView = { id: "review-id", kind: "refine_spec", label: "Review", target: null };

it("dispatches the real checklist offers for a done plan", () => {
  const store = createStore();
  const round: OfferView = {
    id: "round-id",
    kind: "new_metaplan_round",
    label: "Round",
    target: featurePath,
  };
  store.set(checklistAtom, {
    ...checklistViewSchema.parse(fixture.checklists[0]),
    primary: review,
    secondary: [round],
  });
  const handler = vi.fn();
  const unregisterReview = registerOfferAction("refine_spec", handler);
  const unregisterRound = registerOfferAction("new_metaplan_round", handler);
  try {
    render(
      <Provider store={store}>
        <PlanRunActions path={featurePath} state="done" />
      </Provider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Compare spec with build" }));
    fireEvent.click(screen.getByRole("button", { name: "New metaplan round" }));
    expect(handler).toHaveBeenNthCalledWith(1, review);
    expect(handler).toHaveBeenNthCalledWith(2, round);
  } finally {
    unregisterReview();
    unregisterRound();
  }
});

it("offers a done standalone plan nothing from the last visited feature's checklist", () => {
  const store = createStore();
  const round: OfferView = { ...review, id: "round-id", kind: "new_metaplan_round" };
  store.set(checklistAtom, {
    ...checklistViewSchema.parse(fixture.checklists[0]),
    primary: review,
    secondary: [round],
  });
  const { container } = render(
    <Provider store={store}>
      <PlanRunActions path={path} state="done" />
    </Provider>,
  );
  expect(container.innerHTML).toBe("");
});

it("renders no button for a done-plan offer that does not exist", () => {
  const { container } = render(
    <Provider store={createStore()}>
      <PlanRunActions path={path} state="done" />
    </Provider>,
  );
  expect(container.innerHTML).toBe("");
  expect(screen.queryByRole("button")).toBeNull();
  expect(screen.queryByText("No offer is available")).toBeNull();
});

it.each(["running", "retired"] as const)("renders no actions for a %s plan", (state) => {
  const { container } = render(
    <Provider store={createStore()}>
      <PlanRunActions path={path} state={state} />
    </Provider>,
  );
  expect(container.innerHTML).toBe("");
});
