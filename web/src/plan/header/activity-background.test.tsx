import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { connectBackground, stubFetch, TASKS } from "@/plan/chat/background-feed-kit";
import { snapshotAtom } from "@/plan/chat/model";
import { fixture } from "@/plan/chat/transcript-test-kit";
import { ActivityPill } from "./activity";

let disposers: Array<() => void> = [];

beforeEach(stubFetch);

afterEach(() => {
  cleanup();
  disposers.splice(0).forEach((dispose) => dispose());
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

/** The sample pill over a store holding the fixture's chat in `state`, fed by the fake socket. */
async function mountPill(state: "working" | "idle", compact = false) {
  const store = createStore();
  store.set(snapshotAtom, {
    ...fixture,
    previous: null,
    queue: [],
    turns: [],
    session: { ...fixture.session!, state },
  });
  const background = await connectBackground(store);
  disposers.push(background.dispose);
  render(
    <Provider store={store}>
      <MemoryRouter>
        <ActivityPill compact={compact} sample />
      </MemoryRouter>
    </Provider>,
  );
  return background.feed;
}

const unended = TASKS.filter((task) => ["running", "stalled"].includes(task.state));
const ended = TASKS.map((task) => ({ ...task, state: "done" as const }));

it("lists a background row while a task is unended, in any session state, and clears it when the last ends", async () => {
  const feed = await mountPill("working");
  expect(screen.getByRole("button", { name: "Activity: idle" })).toBeTruthy();

  feed(TASKS);
  const pill = await screen.findByRole("button", { name: "3 background tasks" });
  expect(pill.textContent).toBe("3 background tasks");
  fireEvent.click(pill);
  const list = await screen.findByRole("list", { name: "In progress" });
  expect(within(list).getAllByRole("listitem").length).toBe(1);
  expect(within(list).getByText("3 background tasks")).toBeTruthy();

  feed(unended.slice(0, 1));
  await screen.findByRole("button", { name: "1 background task" });
  feed(ended);
  await screen.findByRole("button", { name: "Activity: idle" });
});

it("draws the status dot, never the roundel, for the background row", async () => {
  const feed = await mountPill("working");
  feed(TASKS);
  const pill = await screen.findByRole("button", { name: "3 background tasks" });
  expect(
    pill.querySelector('.activity-glyph[data-kind="background"] .activity-glyph__led'),
  ).not.toBeNull();
  expect(pill.querySelector(".aurora-roundel")).toBeNull();
});

it("leaves the background row out of the in-progress count", async () => {
  const feed = await mountPill("idle");
  feed(TASKS);
  // The idle chat is the one thing in progress; the three tasks are a second row, not three more.
  const pill = await screen.findByRole("button", { name: "1 in progress: Waiting for you" });
  expect(pill.textContent).toBe("1 in progress");
  fireEvent.click(pill);
  const list = await screen.findByRole("list", { name: "In progress" });
  expect(within(list).getAllByRole("listitem").length).toBe(2);
  expect(within(list).getByText("3 background tasks")).toBeTruthy();
});

it("shows only the dot, with the words as its name, in the compact pill", async () => {
  const feed = await mountPill("working", true);
  feed(TASKS);
  const pill = await screen.findByRole("button", { name: "3 background tasks" });
  expect(pill.textContent).toBe("");
  expect(pill.querySelector(".activity-glyph__led")).not.toBeNull();
});
