import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { afterEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import hello from "@/plan/api/fixtures/hello.json";
import { pressureDialogAtom } from "@/plan/offers/actions-pressure";
import { runDialogAtom } from "@/plan/offers/actions-run";
import optionsFixture from "@/plan/api/fixtures/pressure-options.json";
import { pressureOptionsSchema } from "@/plan/api/schema-pressure";
import {
  installPressureSubscription,
  pressureOptionsAtom,
} from "@/plan/chat/pressure/use-pressure";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";
import { PressureTestAction, PressureTestDetail, StartRunButton } from "./plan-run-actions";

const path = "doc/plans/PLAN-example.md";

/** The pressure row's end: the control and the detail line under the words. */
function PressureRow(props: { active?: boolean }) {
  return (
    <>
      <PressureTestAction path={path} date={null} active={props.active} />
      <PressureTestDetail />
    </>
  );
}

const START = "Pressure-test this plan";

/** Opens the menu from the state button and returns it. */
async function openMenu(name: string | RegExp = "Not pressure-tested") {
  fireEvent.click(screen.getByRole("button", { name }));
  return within(await screen.findByRole("dialog", { name }));
}

/** The menu's start choice; `description` is its note, or the reason that blocks it. */
async function startChoice(description?: string) {
  return (await openMenu()).getByRole("button", { name: START, description });
}
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

it("opens a menu from the state button, whose choice starts the test", async () => {
  const store = createStore();
  render(
    <Provider store={store}>
      <PressureRow />
    </Provider>,
  );
  const trigger = screen.getByRole("button", { name: "Not pressure-tested" });
  expect(trigger.getAttribute("aria-haspopup")).toBe("dialog");
  expect(trigger.getAttribute("aria-expanded")).toBe("false");
  expect(trigger.querySelector("svg.trace-menu-chevron")).not.toBeNull();

  fireEvent.click(trigger);
  // The press opens the menu; it starts nothing by itself.
  expect(store.get(pressureDialogAtom).open).toBe(false);
  expect(trigger.getAttribute("aria-expanded")).toBe("true");
  const menu = within(await screen.findByRole("dialog", { name: "Not pressure-tested" }));
  expect(menu.getAllByRole("button").map((item) => item.textContent)).toEqual([START]);

  fireEvent.click(
    menu.getByRole("button", {
      name: START,
      description: "Opens the pressure test, where you start it.",
    }),
  );
  expect(store.get(pressureDialogAtom)).toEqual({ open: true, plan: path });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(trigger.getAttribute("aria-expanded")).toBe("false");
});

it("Escape closes the pressure menu and returns focus to the state button", async () => {
  render(
    <Provider store={createStore()}>
      <PressureRow />
    </Provider>,
  );
  const trigger = screen.getByRole("button", { name: "Not pressure-tested" });
  fireEvent.click(trigger);
  fireEvent.keyDown(await screen.findByRole("dialog"), { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(document.activeElement).toBe(trigger);
});

it("blocks the menu's choice with the authoring reason", async () => {
  const store = createStore();
  holdAuthoring(store);
  store.set(authoringAtom, {
    holder: { public_id: "other", name: "Ada", away_since: null, frees_at: null },
  });
  render(
    <Provider store={store}>
      <PressureRow />
    </Provider>,
  );
  const choice = await startChoice("Ada holds authoring");
  expect(choice.getAttribute("aria-disabled")).toBe("true");
  fireEvent.click(choice);
  expect(store.get(pressureDialogAtom).open).toBe(false);
});

it("blocks the menu's choice while a test runs on the plan", async () => {
  const store = createStore();
  render(
    <Provider store={store}>
      <PressureRow active />
    </Provider>,
  );
  const menu = await openMenu("Being pressure-tested");
  const choice = menu.getByRole("button", {
    name: START,
    description: "A pressure test is running on this plan.",
  });
  expect(choice.getAttribute("aria-disabled")).toBe("true");
  fireEvent.click(choice);
  expect(store.get(pressureDialogAtom).open).toBe(false);
});

it("opens the run dialog from StartRunButton for the authoring holder", () => {
  const store = createStore();
  holdAuthoring(store);
  render(
    <Provider store={store}>
      <StartRunButton path={path} />
    </Provider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Start a run" }));
  expect(store.get(runDialogAtom)).toEqual({ open: true, plan: path });
});

it("disables Pressure-test with the server reason when the options say unavailable", async () => {
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
      <PressureRow />
    </Provider>,
  );
  const choice = await startChoice("Codex is not installed");
  expect(choice.getAttribute("aria-disabled")).toBe("true");
  fireEvent.click(choice);
  expect(store.get(pressureDialogAtom).open).toBe(false);
});

it("disables Start a run for a browser that does not hold authoring, since the run routes need the holder", async () => {
  const store = createStore();
  store.set(serverInfoAtom, { ...hello.server, read_only: [] });
  store.set(youAtom, hello.you);
  store.set(authoringAtom, { holder: null });
  render(
    <Provider store={store}>
      <StartRunButton path={path} />
      <PressureRow />
    </Provider>,
  );

  const start = screen.getByRole("button", {
    name: "Start a run",
    description: "Start a chat to take authoring, then start a run.",
  });
  fireEvent.click(start);

  expect(store.get(runDialogAtom).open).toBe(false);
  // Pressure start takes free authoring itself, so it stays available.
  fireEvent.click(await startChoice());
  expect(store.get(pressureDialogAtom).open).toBe(true);
});

it("falls back to a generic reason when the server is unavailable without giving one", async () => {
  const store = createStore();
  store.set(pressureOptionsAtom, {
    ok: true,
    value: optionsWith({ available: false, disabled_reason: null }),
  });
  render(
    <Provider store={store}>
      <PressureRow />
    </Provider>,
  );

  expect(await startChoice("Pressure-testing is unavailable")).toBeTruthy();
});

it("shows control characters in the options reason as visible markers", async () => {
  const store = createStore();
  store.set(pressureOptionsAtom, {
    ok: true,
    value: optionsWith({ available: false, disabled_reason: "Codex changed: .claude/\u202Ex" }),
  });
  render(
    <Provider store={store}>
      <PressureRow />
    </Provider>,
  );

  expect(await startChoice("Codex changed: .claude/⟨U+202E⟩x")).toBeTruthy();
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
        <PressureRow />
      </Provider>,
    );

    const menu = await openMenu();
    expect(
      menu.getByRole("button", { name: START, description: "Ada holds authoring" }),
    ).toBeTruthy();

    act(() => store.set(authoringAtom, { holder: null }));

    await waitFor(() =>
      expect(menu.getByRole("button", { name: START }).getAttribute("aria-disabled")).toBeNull(),
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
      <PressureRow />
    </Provider>,
  );
  expect(screen.getByRole("region", { name: "Codex configuration change (plan)" })).toBeTruthy();
  const menu = await openMenu();
  expect(menu.getByRole("button", { name: START, description: blocked.reason })).toBeTruthy();

  fireEvent.click(screen.getByRole("button", { name: "I have checked these files" }));

  await waitFor(() => expect(screen.queryByRole("region")).toBeNull());
  expect(menu.getByRole("button", { name: START }).getAttribute("aria-disabled")).toBeNull();
  expect(fetch).toHaveBeenCalledWith(
    "/api/pressure/confirm-config",
    expect.objectContaining({ method: "POST" }),
  );
});
