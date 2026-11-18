import { fireEvent, screen, waitFor } from "@testing-library/react";
import { expect, it } from "vitest";

import { checkpointAtom } from "./checkpoint";
import {
  A,
  ADDED,
  checkpoint,
  current,
  failures,
  fetchMock,
  installChangedSinceLifecycle,
  mount,
  openList,
  replies,
} from "./changed-since-test-kit";
import { hashesAtom } from "./live";

installChangedSinceLifecycle();

// A commit sends no `doc_changed`, so the page's cached head is older than the server's.
const fresh = { head: "c3", docs: { ...current.docs, [ADDED]: "added-again" } };

function markAllSeen() {
  fireEvent.click(screen.getByRole("button", { name: "Mark all seen" }));
}

function hashRequests() {
  return fetchMock.mock.calls.filter(([url]) => url === "/api/hashes").length;
}

/** Holds the next request (the hashes fetch of Mark all seen) until the returned function runs. */
function holdNextRequest() {
  let release!: (body: unknown) => void;
  fetchMock.mockImplementationOnce(
    () =>
      new Promise<Response>((resolve) => {
        release = (body) => resolve(new Response(JSON.stringify(body)));
      }),
  );
  return (body: unknown) => release(body);
}

it("stores the head and hashes of a fresh /api/hashes, not the cached ones", async () => {
  replies["/api/hashes"] = fresh;
  replies["/api/compare?commit=c3"] = { commit: "c3", available: true, docs: [] };
  const { store } = mount();
  await openList();
  expect(store.get(hashesAtom)?.head).toBe("c2");

  markAllSeen();

  expect(await screen.findByText("Nothing changed since you looked.")).toBeTruthy();
  expect(store.get(checkpointAtom)).toEqual({ commit: "c3", hashes: fresh.docs });
  expect(store.get(hashesAtom)).toEqual(fresh);
});

it("shows a pending state and ignores a second press while the hashes load", async () => {
  const { store } = mount();
  await openList();
  const release = holdNextRequest();
  const button = screen.getByRole("button", { name: "Mark all seen" });
  button.focus();

  markAllSeen();

  const pending = await screen.findByRole("button", { name: "Marking all seen…" });
  // `aria-disabled`, not `disabled`: a disabled button cannot keep the focus it holds.
  expect(pending).toBe(button);
  expect(pending.hasAttribute("disabled")).toBe(false);
  expect(pending.getAttribute("aria-disabled")).toBe("true");
  expect(document.activeElement).toBe(pending);
  fireEvent.click(pending);
  expect(hashRequests()).toBe(1);
  expect(store.get(checkpointAtom)).toEqual(checkpoint);

  release(current);

  await waitFor(() =>
    expect(store.get(checkpointAtom)).toEqual({ commit: "c2", hashes: current.docs }),
  );
  // Nothing is left to mark: the button goes with the list, and its focus moves to the way back.
  expect(screen.queryByRole("button", { name: /Mark all seen|Marking all seen/ })).toBeNull();
  const back = await screen.findByRole("link", { name: "Go to Home" });
  expect(document.activeElement).toBe(back);
});

it("a doc_changed reload that starts after Mark all seen does not land before it", async () => {
  const edited = { head: "c2", docs: { ...current.docs, [A]: "edited-in-chat" } };
  replies["/api/compare?commit=c2"] = { commit: "c2", available: true, docs: [] };
  const { store, emit } = mount({ live: true });
  await openList();
  const release = holdNextRequest();
  markAllSeen();
  await screen.findByRole("button", { name: "Marking all seen…" });
  // The server moves on while Mark all seen is in flight: its response is the older one.
  replies["/api/hashes"] = edited;

  emit({ type: "doc_changed", path: A, hash: null });
  release(current);

  await waitFor(() =>
    expect(store.get(checkpointAtom)).toEqual({ commit: "c2", hashes: current.docs }),
  );
  await waitFor(() => expect(store.get(hashesAtom)).toEqual(edited));
  expect(await screen.findByRole("button", { name: `Show diff for ${A}` })).toBeTruthy();
  expect(hashRequests()).toBe(3);
});

it("shows the error and stores nothing when the hashes fail, then a retry works", async () => {
  failures["/api/hashes"] = { status: 500, message: "Hashes are unavailable" };
  const { store } = mount();
  await openList();

  markAllSeen();

  expect(await screen.findByText("Hashes are unavailable")).toBeTruthy();
  expect(store.get(checkpointAtom)).toEqual(checkpoint);
  const retry = screen.getByRole("button", { name: "Mark all seen" }) as HTMLButtonElement;
  expect(retry.disabled).toBe(false);

  delete failures["/api/hashes"];
  markAllSeen();

  await waitFor(() =>
    expect(store.get(checkpointAtom)).toEqual({ commit: "c2", hashes: current.docs }),
  );
  expect(screen.queryByText("Hashes are unavailable")).toBeNull();
});

it("leaving the page before the hashes arrive stores nothing", async () => {
  const { store, view } = mount();
  await openList();
  const release = holdNextRequest();
  markAllSeen();
  await screen.findByRole("button", { name: "Marking all seen…" });

  view.unmount();
  release(fresh);
  await new Promise((resolve) => setTimeout(resolve, 0));

  expect(store.get(checkpointAtom)).toEqual(checkpoint);
});
