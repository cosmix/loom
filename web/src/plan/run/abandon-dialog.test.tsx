import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import hello from "@/plan/api/fixtures/hello.json";
import runStateFixture from "@/plan/api/fixtures/run-state.json";
import { runStateSchema, type RunStateView, type UnmergedBranch } from "@/plan/api/schema-run";
import { authoringAtom, connectionAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";

import { AbandonDialog } from "./abandon-dialog";

type Viewer = "holder" | "other" | "read_only" | "unclaimed";
const fetchMock = vi.fn<typeof fetch>();
const deleteName = /^Delete \d+ branch/;

function jsonReply(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function runState(branches?: UnmergedBranch[]): RunStateView {
  const state = runStateSchema.parse(runStateFixture);
  if (branches !== undefined && state.abandon) state.abandon.unmerged_branches = branches;
  return state;
}

function show(state = runState(), viewer: Viewer = "holder") {
  const store = createStore();
  store.set(serverInfoAtom, {
    ...hello.server,
    read_only:
      viewer === "read_only"
        ? [{ what: "tmux", message: "tmux is not installed", fix: "Install tmux" }]
        : [],
  });
  store.set(youAtom, hello.you);
  store.set(authoringAtom, {
    holder:
      viewer === "unclaimed"
        ? null
        : {
            ...hello.you,
            ...(viewer === "other" ? { public_id: "bbbbbbbbbbbbbbbb", name: "Dana" } : {}),
            away_since: null,
            frees_at: null,
          },
  });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: 0 });
  initClient(store);
  const view = (value: RunStateView) => (
    <Provider store={store}>
      <MemoryRouter>
        <AbandonDialog state={value} />
      </MemoryRouter>
    </Provider>
  );
  const result = render(view(state));
  return { store, rerenderState: (value: RunStateView) => result.rerender(view(value)) };
}

function abandonBodies() {
  return fetchMock.mock.calls
    .filter(([input, init]) => String(input) === "/api/run/abandon" && init?.method === "POST")
    .map(([, init]) => JSON.parse(String(init?.body)) as unknown);
}

function openFirst() {
  fireEvent.click(screen.getByRole("button", { name: "Abandon run" }));
  return screen.getByRole("dialog", { name: "Abandon run" });
}

function openSecond() {
  fireEvent.click(within(openFirst()).getByRole("button", { name: "Abandon run" }));
  return screen.getByRole("dialog", { name: "Delete branches with unmerged commits?" });
}

beforeEach(() => {
  fetchMock.mockReset();
  fetchMock.mockImplementation(async () =>
    jsonReply({ to: "doc/plans/archive/RETIRED-PLAN-example.md" }),
  );
  vi.stubGlobal("fetch", fetchMock);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("renders nothing when there is no abandon or finish-abandon state", () => {
  show({ ...runState(), abandon: null, finish_abandon: null });
  expect(screen.queryByRole("button")).toBeNull();
  expect(screen.queryByRole("status")).toBeNull();
});

it("names the plan, lost state, and merged and unmerged stages before requesting anything", () => {
  show();
  const dialog = within(openFirst());

  expect(dialog.getByText("Abandon the run for doc/plans/PLAN-example.md?")).toBeTruthy();
  expect(dialog.getByText(/doc\/plans\/archive\/RETIRED-PLAN-<slug>\.md/)).toBeTruthy();
  expect(dialog.getByText(/metaplan entry closes/)).toBeTruthy();
  expect(dialog.getByText(/daemon stops.*running stage session is killed/)).toBeTruthy();
  expect(dialog.getByText(/\.loom\/work.*stage state, memory journals/)).toBeTruthy();
  expect(dialog.getByText(/\.worktrees\/.*loom\/\* branch/)).toBeTruthy();
  expect(dialog.getByRole("heading", { name: "Merged stages — work stays on main" })).toBeTruthy();
  expect(dialog.getByText("foundation")).toBeTruthy();
  expect(dialog.getByRole("heading", { name: "Unmerged stages" })).toBeTruthy();
  expect(dialog.getByText("implementation")).toBeTruthy();
  expect(fetchMock).not.toHaveBeenCalled();
});

it("abandons after the first confirmation with an empty list when there are no unmerged branches", async () => {
  show(runState([]));
  fireEvent.click(within(openFirst()).getByRole("button", { name: "Abandon run" }));

  await waitFor(() => expect(abandonBodies()).toEqual([{ confirmed_unmerged: [] }]));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(screen.queryByRole("button", { name: deleteName })).toBeNull();
});

it("requires a separate confirmation showing each branch and ahead count", async () => {
  show(
    runState([
      { branch: "loom/implementation", ahead: 2 },
      { branch: "loom/tests", ahead: 7 },
    ]),
  );
  const second = within(openSecond());

  expect(screen.queryByRole("dialog", { name: "Abandon run" })).toBeNull();
  expect(second.getByText("loom/implementation: 2 commits ahead of main")).toBeTruthy();
  expect(second.getByText("loom/tests: 7 commits ahead of main")).toBeTruthy();
  expect(fetchMock).not.toHaveBeenCalled();
  fireEvent.click(second.getByRole("button", { name: "Delete 2 branches with unmerged commits" }));

  await waitFor(() =>
    expect(abandonBodies()).toEqual([
      {
        confirmed_unmerged: [
          { branch: "loom/implementation", ahead: 2 },
          { branch: "loom/tests", ahead: 7 },
        ],
      },
    ]),
  );
});

it("posts the displayed snapshot even when live state changes before confirmation", async () => {
  const { rerenderState } = show();
  openSecond();
  rerenderState(runState([{ branch: "loom/new-stage", ahead: 12 }]));

  expect(screen.getByText("loom/implementation: 2 commits ahead of main")).toBeTruthy();
  expect(screen.queryByText(/loom\/new-stage/)).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Delete 1 branch with unmerged commits" }));

  await waitFor(() =>
    expect(abandonBodies()).toEqual([
      {
        confirmed_unmerged: [{ branch: "loom/implementation", ahead: 2 }],
      },
    ]),
  );
});

it.each(["first", "branches"])(
  "cancelling the %s confirmation closes everything and sends no request",
  async (step) => {
    show();
    const dialog = step === "first" ? openFirst() : openSecond();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.getByRole("status").textContent).toContain(
      "You can merge or push those branches first and come back.",
    );
    expect(abandonBodies()).toEqual([]);
    openFirst();
    expect(
      screen.queryByText("You can merge or push those branches first and come back."),
    ).toBeNull();
    expect(fetchMock).not.toHaveBeenCalled();
  },
);

it("reopens the second confirmation with a fresh 409 list and waits for consent again", async () => {
  fetchMock.mockResolvedValueOnce(
    jsonReply(
      {
        error: "unmerged_branches",
        message: "The branch list changed",
        unmerged_branches: [{ branch: "loom/new-stage", ahead: 3 }],
      },
      409,
    ),
  );
  show();
  fireEvent.click(within(openSecond()).getByRole("button", { name: deleteName }));

  expect(await screen.findByText("loom/new-stage: 3 commits ahead of main")).toBeTruthy();
  expect(screen.queryByText("loom/implementation: 2 commits ahead of main")).toBeNull();
  expect(fetchMock).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByRole("button", { name: "Delete 1 branch with unmerged commits" }));

  await waitFor(() =>
    expect(abandonBodies()).toEqual([
      { confirmed_unmerged: [{ branch: "loom/implementation", ahead: 2 }] },
      { confirmed_unmerged: [{ branch: "loom/new-stage", ahead: 3 }] },
    ]),
  );
});

it("requires the second confirmation when a 409 finds new unmerged branches after an empty listing", async () => {
  fetchMock.mockResolvedValueOnce(
    jsonReply(
      {
        error: "unmerged_branches",
        message: "Confirm the new list",
        unmerged_branches: [{ branch: "loom/raced", ahead: 1 }],
      },
      409,
    ),
  );
  show(runState([]));
  fireEvent.click(within(openFirst()).getByRole("button", { name: "Abandon run" }));

  expect(await screen.findByText("loom/raced: 1 commit ahead of main")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(abandonBodies()).toEqual([{ confirmed_unmerged: [] }]);
  expect(screen.queryByRole("dialog")).toBeNull();
});

it.each([409, 500])("shows other server errors as text for HTTP %i", async (status) => {
  fetchMock.mockResolvedValueOnce(
    jsonReply({ error: "refused", message: "<img src=x onerror=alert(1)>" }, status),
  );
  show(runState([]));
  fireEvent.click(within(openFirst()).getByRole("button", { name: "Abandon run" }));

  expect(await screen.findByText("<img src=x onerror=alert(1)>")).toBeTruthy();
  expect(screen.queryByRole("img")).toBeNull();
  expect(screen.getByRole("dialog", { name: "Abandon run" })).toBeTruthy();
});

it("renders untrusted stage and branch names as text", () => {
  const state = runState([{ branch: "loom/<img src=x>", ahead: 1 }]);
  state.abandon!.merged_stages = ["<script>stage</script>"];
  show(state);
  const first = openFirst();
  expect(within(first).getByText("<script>stage</script>")).toBeTruthy();
  fireEvent.click(within(first).getByRole("button", { name: "Abandon run" }));

  expect(screen.getByText("loom/<img src=x>: 1 commit ahead of main")).toBeTruthy();
  expect(document.querySelector("script, img")).toBeNull();
});

it.each<Viewer>(["other", "read_only", "unclaimed"])(
  "hides Abandon run for %s viewers",
  (viewer) => {
    show(runState(), viewer);
    expect(screen.queryByRole("button", { name: "Abandon run" })).toBeNull();
    expect(fetchMock).not.toHaveBeenCalled();
  },
);

it("closes the confirmation without a request when authoring is lost", () => {
  const { store } = show();
  openSecond();
  act(() => store.set(authoringAtom, { holder: null }));

  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.queryByRole("button", { name: "Abandon run" })).toBeNull();
  expect(fetchMock).not.toHaveBeenCalled();
});

it("keeps blocked Abandon focusable with an accessible reason while a run job is busy", () => {
  show({ ...runState(), busy: "start" });
  const button = screen.getByRole("button", {
    name: "Abandon run",
    description: "A run action is in progress",
  });

  expect(button.getAttribute("aria-disabled")).toBe("true");
  expect(button.hasAttribute("disabled")).toBe(false);
  fireEvent.click(button);
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(fetchMock).not.toHaveBeenCalled();
});

it("posts finish-abandon for a holder and reports completion", async () => {
  show({ ...runState(), abandon: null, finish_abandon: "doc/plans/IN_PROGRESS-PLAN-example.md" });
  expect(screen.getByText("This plan's run was removed but the plan was not retired")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Finish abandoning" }));

  await waitFor(() =>
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/run/finish-abandon",
      expect.objectContaining({
        method: "POST",
        body: "{}",
      }),
    ),
  );
  expect(await screen.findByText("Plan retired")).toBeTruthy();
});

it.each<Viewer>(["other", "read_only", "unclaimed"])(
  "shows recovery information but hides Finish abandoning for %s viewers",
  (viewer) => {
    show(
      { ...runState(), abandon: null, finish_abandon: "doc/plans/IN_PROGRESS-PLAN-example.md" },
      viewer,
    );

    expect(
      screen.getByText("This plan's run was removed but the plan was not retired"),
    ).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Finish abandoning" })).toBeNull();
    expect(fetchMock).not.toHaveBeenCalled();
  },
);

it("shows a finish-abandon failure and allows retry", async () => {
  fetchMock.mockResolvedValueOnce(
    jsonReply({ error: "refused", message: "Plan could not be retired" }, 409),
  );
  show({ ...runState(), abandon: null, finish_abandon: "doc/plans/IN_PROGRESS-PLAN-example.md" });
  fireEvent.click(screen.getByRole("button", { name: "Finish abandoning" }));

  expect(await screen.findByText("Plan could not be retired")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Finish abandoning" }));
  expect(await screen.findByText("Plan retired")).toBeTruthy();
});
