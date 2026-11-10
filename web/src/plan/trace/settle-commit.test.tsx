import { act, cleanup, fireEvent, renderHook, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { connectionEpochAtom, planSocketAtom } from "@/plan/state/atoms";

import {
  COMMIT,
  HASH,
  JOB,
  S03,
  deliver,
  fakeSocket,
  kit,
  mount,
  patchDoc,
  resetKit,
  settlePosts,
} from "./components-kit";
import { SettleControl } from "./settle-control";
import { COMMITTED_NOTE_MS, LOST_NOTE, useCommitNote } from "./settle-note";

// The note beside the settle control about the commit, which runs after the settle answers and
// whose end arrives as a `settle_commit` frame on the plan socket.

beforeEach(resetKit);

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

/** The `settle_commit` frame of the answer's job for section 03, committed, with `patch` over it. */
function frame(patch: Record<string, unknown> = {}) {
  return {
    type: "settle_commit",
    job: JOB,
    path: S03,
    state: "committed",
    commit: COMMIT,
    reason: null,
    ...patch,
  };
}

/** Hold the settle answer until the returned `answer` sends it, naming the job `JOB`. */
function holdAnswer(): () => void {
  let release = (_value: Response) => {};
  const held = new Promise<Response>((resolve) => {
    release = resolve;
  });
  const fetch = globalThis.fetch;
  vi.stubGlobal("fetch", (input: RequestInfo | URL, init?: RequestInit) =>
    init?.method === "POST" ? held : fetch(input, init),
  );
  const body = { result: "saved", hash: "sha256:after", validation: null };
  const headers = { "content-type": "application/json" };
  return () =>
    release(
      new Response(JSON.stringify({ ...body, commit: { state: "pending", job: JOB } }), {
        headers,
      }),
    );
}

/** Mount section 03's control, press `name` (and `then`, a menu item), and wait for the post. */
async function press(name: string, then?: string) {
  const store = mount(<SettleControl feature="demo" path={S03} hash={HASH} />);
  fireEvent.click(await screen.findByRole("button", { name }));
  if (then) fireEvent.click(await screen.findByRole("button", { name: then }));
  await waitFor(() => expect(settlePosts()).toHaveLength(1));
  return store;
}

it("Committing… stands beside the control while the commit runs, then the commit for a moment", async () => {
  await press("Mark settled");
  const status = await screen.findByRole("status");
  expect(status.textContent).toBe("Committing…");
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  try {
    deliver(frame());
    expect(screen.getByRole("status")).toBe(status);
    expect(status.textContent).toBe("Committed 0a1b2c3");
    await act(async () => {
      await vi.advanceTimersByTimeAsync(COMMITTED_NOTE_MS);
    });
  } finally {
    vi.useRealTimers();
  }
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
});

it("a commit that did not happen says why, from the broadcast", async () => {
  await press("Mark settled");
  await screen.findByText("Committing…");
  deliver(frame({ state: "not_committed", commit: null, reason: "a merge is in progress" }));
  const notice = await screen.findByRole("alert");
  expect(notice.textContent).toBe("Settled, but not committed: a merge is in progress");
  expect(screen.queryByText("Committing…")).toBeNull();
});

it("an unsettle the server could not commit says so", async () => {
  patchDoc(S03, { settle: "settled" });
  await press("Settled", "Unsettle");
  const reason = "git commit exited with 1: docs are frozen";
  deliver(frame({ state: "not_committed", commit: null, reason }));
  expect((await screen.findByRole("alert")).textContent).toBe(
    "Unsettled, but not committed: git commit exited with 1: docs are frozen",
  );
});

it("a commit whose hook changed the document says to settle the hook's version", async () => {
  await press("Mark settled");
  const reason =
    "A commit hook changed the document after it was settled. Settle this version to record it.";
  deliver(frame({ reason }));
  expect((await screen.findByRole("alert")).textContent).toBe(`Settled and committed. ${reason}`);
});

it("nothing to commit leaves no note", async () => {
  await press("Mark settled");
  await screen.findByText("Committing…");
  deliver(frame({ state: "unchanged", commit: null }));
  await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
  expect(screen.queryByRole("alert")).toBeNull();
});

it("an end that arrives before the answer is not lost", async () => {
  const answer = holdAnswer();
  mount(<SettleControl feature="demo" path={S03} hash={HASH} />);
  fireEvent.click(await screen.findByRole("button", { name: "Mark settled" }));
  deliver(frame({ state: "not_committed", commit: null, reason: "a merge is in progress" }));
  answer();
  const notice = await screen.findByRole("alert");
  expect(notice.textContent).toBe("Settled, but not committed: a merge is in progress");
  expect(screen.queryByText("Committing…")).toBeNull();
});

it("the end of another job, of another server's job, or of another document, is not this press's", async () => {
  await press("Mark settled");
  deliver(frame({ job: "3f9a1c2e-2", state: "not_committed", commit: null, reason: "not mine" }));
  deliver(frame({ job: "77777777-1", state: "not_committed", commit: null, reason: "old server" }));
  deliver(frame({ path: "doc/product/demo/section_02.md", state: "not_committed", reason: "no" }));
  expect(screen.getByRole("status").textContent).toBe("Committing…");
  expect(screen.queryByRole("alert")).toBeNull();
});

it("a reconnect while the commit runs says its result was lost, until the end arrives", async () => {
  const store = await press("Mark settled");
  await screen.findByText("Committing…");
  act(() => store.set(connectionEpochAtom, (epoch) => epoch + 1));
  expect((await screen.findByRole("alert")).textContent).toBe(LOST_NOTE);
  deliver(frame());
  expect((await screen.findByRole("status")).textContent).toBe("Committed 0a1b2c3");
  expect(screen.queryByRole("alert")).toBeNull();
});

it("a reconnect before the answer names the job says the result was lost", async () => {
  const answer = holdAnswer();
  const store = mount(<SettleControl feature="demo" path={S03} hash={HASH} />);
  fireEvent.click(await screen.findByRole("button", { name: "Mark settled" }));
  act(() => store.set(connectionEpochAtom, (epoch) => epoch + 1));
  expect(screen.queryByRole("alert")).toBeNull();
  answer();
  expect((await screen.findByRole("alert")).textContent).toBe(LOST_NOTE);
});

it("a commit the server could not queue says why at once", async () => {
  const reason = "the commit worker could not start";
  kit.settleAnswers = [
    {
      status: 200,
      body: {
        result: "saved",
        hash: "sha256:after",
        validation: null,
        commit: { state: "not_committed", reason },
      },
    },
  ];
  await press("Mark settled");
  expect((await screen.findByRole("alert")).textContent).toBe(
    `Settled, but not committed: ${reason}`,
  );
});

/** The job of a second press on section 03, queued behind `JOB`. */
const SECOND = "3f9a1c2e-2";

/** Section 03's commit note with two presses waiting: a settle (`JOB`), then an unsettle. */
function twoPresses() {
  const store = createStore();
  store.set(planSocketAtom, fakeSocket() as never);
  const wrapper = ({ children }: { children: ReactNode }) => (
    <Provider store={store}>{children}</Provider>
  );
  const { result } = renderHook(() => useCommitNote(S03), { wrapper });
  act(() => result.current.expect(false));
  act(() => result.current.started({ state: "pending", job: JOB }));
  act(() => result.current.expect(true));
  act(() => result.current.started({ state: "pending", job: SECOND }));
  expect(result.current.note).toEqual({ kind: "pending" });
  return result;
}

it("a second press keeps waiting for the first commit and shows its failure", () => {
  const note = twoPresses();
  deliver(frame({ state: "not_committed", commit: null, reason: "docs are frozen" }));
  expect(note.current.note).toEqual({
    kind: "failed",
    message: "Settled, but not committed: docs are frozen",
  });
  deliver(frame({ job: SECOND, state: "not_committed", commit: null, reason: "still frozen" }));
  expect(note.current.note).toEqual({
    kind: "failed",
    message: "Unsettled, but not committed: still frozen",
  });
});

it("a first commit made while a second press waits leaves the note on the second", () => {
  const note = twoPresses();
  deliver(frame());
  expect(note.current.note).toEqual({ kind: "pending" });
  deliver(frame({ job: SECOND }));
  expect(note.current.note).toEqual({ kind: "committed", commit: COMMIT });
});
