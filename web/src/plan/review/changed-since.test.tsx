import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";

import { storageKey } from "@/plan/chat/storage";
import { openDiff } from "@/plan/diff/open-diff";
import { serverInfoAtom } from "@/plan/state/atoms";
import { checkpointAtom } from "./checkpoint";
import {
  A,
  ADDED,
  GONE,
  REF,
  SAME,
  checkpoint,
  compareRef,
  current,
  failures,
  fetchMock,
  installChangedSinceLifecycle,
  mount,
  openList,
  replies,
  server,
} from "./changed-since-test-kit";

vi.mock("@/plan/diff/diff-view", () => ({
  DiffView: ({ before, after }: { before: string | null; after: string | null }) => (
    <div role="group" aria-label="Loaded diff">
      {JSON.stringify({ before, after })}
    </div>
  ),
}));

installChangedSinceLifecycle();
it("lists added, changed and removed documents, never unchanged ones", async () => {
  mount();
  const list = await openList();

  expect(within(list).queryByText(SAME)).toBeNull();
  expect(within(list).getByText(A).closest("li")?.textContent).toContain("changed");
  expect(within(list).getByText(ADDED).closest("li")?.textContent).toContain("added");
  expect(within(list).getByText(GONE).closest("li")?.textContent).toContain("removed");
  // The open dialog hides the trigger from the accessibility tree, hence `hidden`.
  expect(
    screen.getByRole("button", { name: /Changed since you looked · 3/, hidden: true }),
  ).toBeTruthy();
});

it("opening a row's diff marks it seen", async () => {
  const { store } = mount();
  await openList();

  fireEvent.click(screen.getByRole("button", { name: `Show diff for ${A}` }));
  await screen.findByRole("group", { name: "Loaded diff" });

  expect(store.get(checkpointAtom)).toEqual({
    commit: "c1",
    hashes: { [A]: "new", [GONE]: "gone", [SAME]: "same" },
  });
  expect(within(screen.getByRole("list")).queryByText(A)).toBeNull();
});

it("diff before from at-commit", async () => {
  mount();
  await openList();

  fireEvent.click(screen.getByRole("button", { name: `Show diff for ${A}` }));

  expect((await screen.findByRole("group", { name: "Loaded diff" })).textContent).toBe(
    JSON.stringify({ before: "committed text", after: "current text" }),
  );
  expect(fetchMock).toHaveBeenCalledWith(
    `/api/at-commit?commit=c1&path=${encodeURIComponent(A)}`,
    expect.anything(),
  );
  expect(fetchMock).toHaveBeenCalledWith("/api/doc?path=product%2Fa.md", expect.anything());
});

it("ref comparison lists and does not store", async () => {
  const { store, projectKey } = mount();
  const stored = localStorage.getItem(storageKey(projectKey, "checkpoint"));
  await openList();

  await compareRef();
  expect(within(screen.getByRole("list")).queryByText(A)).toBeNull();
  fireEvent.click(await screen.findByRole("button", { name: `Show diff for ${REF}` }));
  await screen.findByRole("group", { name: "Loaded diff" });

  expect(fetchMock).toHaveBeenCalledWith(
    `/api/at-commit?commit=base&path=${encodeURIComponent(REF)}`,
    expect.anything(),
  );
  expect(store.get(checkpointAtom)).toEqual(checkpoint);
  expect(localStorage.getItem(storageKey(projectKey, "checkpoint"))).toBe(stored);
  expect(localStorage.length).toBe(1);
});

it("uncommitted note on row", async () => {
  mount();
  const list = await openList();

  expect(within(list).getByText(A).closest("li")?.textContent).toContain(
    "uncommitted edits at the checkpoint",
  );
  expect(within(list).getByText(A).closest("li")?.textContent).toContain("committed text");
});

it("unavailable commit offers only uncommitted and ref diffs", async () => {
  replies["/api/compare?commit=c1"] = { commit: "c1", available: false, docs: [] };
  mount();
  fireEvent.click(screen.getByRole("button", { name: /Changed since you looked/ }));

  await screen.findByText(/The checkpoint commit is unavailable/);
  const list = screen.getByRole("list");
  expect(within(list).getAllByRole("listitem")).toHaveLength(3);
  expect(within(list).queryByRole("button", { name: /Show diff/ })).toBeNull();
  expect(
    within(list).getAllByRole("link", { name: "Uncommitted diff" })[0].getAttribute("href"),
  ).toBe(openDiff("head", A));
  expect(within(list).getAllByText("changed")).toHaveLength(3);
  await compareRef();
  expect(await screen.findByRole("button", { name: `Show diff for ${REF}` })).toBeTruthy();
});

it("limits text shown", async () => {
  mount();
  await openList();

  expect(screen.getByText(/Local storage belongs/).textContent).toMatch(/origin.*port.*browser/);
  expect(screen.getByText(/An unavailable checkpoint commit/)).toBeTruthy();
  expect(screen.getByText(/Uncommitted edits at the checkpoint/)).toBeTruthy();
});

it("resync refetches the compare list", async () => {
  const { listeners, view } = mount();
  await openList();
  await compareRef();
  replies["/api/compare?commit=base"] = {
    commit: "base",
    available: true,
    docs: [{ path: "doc/new.md", change: "added" }],
  };

  act(() => listeners.forEach((handler) => handler()));

  expect(await screen.findByText("doc/new.md")).toBeTruthy();
  expect(screen.queryByText(REF)).toBeNull();
  view.unmount();
  expect(listeners.size).toBe(0);
});

it("checkpoint keyed by project key", async () => {
  const { store, projectKey } = mount();
  const other = { commit: "other", hashes: { ...current.docs } };
  localStorage.setItem(storageKey("another-project", "checkpoint"), JSON.stringify(other));

  act(() => store.set(serverInfoAtom, { ...server, project_key: "another-project" }));

  await waitFor(() =>
    expect(screen.queryByRole("button", { name: /Changed since you looked/ })).toBeNull(),
  );
  expect(store.get(checkpointAtom)).toEqual(other);
  expect(JSON.parse(localStorage.getItem(storageKey(projectKey, "checkpoint"))!)).toEqual(
    checkpoint,
  );
  act(() => store.set(serverInfoAtom, { ...server, project_key: projectKey }));
  expect(await screen.findByRole("button", { name: /Changed since you looked · 3/ })).toBeTruthy();
});

it("first-visit checkpoint set silently", async () => {
  const { store } = mount({ checkpoint: null, live: true });

  await waitFor(() =>
    expect(store.get(checkpointAtom)).toEqual({ commit: "c2", hashes: current.docs }),
  );

  expect(screen.queryByRole("button", { name: /Changed since you looked/ })).toBeNull();
  expect(screen.queryByRole("dialog")).toBeNull();
});

it("resolveRef refusal shown", async () => {
  failures["/api/ref?name=main"] = { status: 404, message: "The ref does not name a known commit" };
  mount();
  await openList();
  fireEvent.change(screen.getByLabelText("Compare with ref"), { target: { value: "main" } });

  fireEvent.click(screen.getByRole("button", { name: "Compare" }));

  expect(await screen.findByText("The ref does not name a known commit")).toBeTruthy();
});

it("compact shows only a dot and count with the full accessible name", () => {
  mount({ compact: true });

  const control = screen.getByRole("button", { name: "Changed since you looked · 3" });
  expect(control.textContent).toBe("● 3");
});

it("a reviewed removal is marked seen without inventing a current hash", async () => {
  const { store } = mount();
  await openList();

  fireEvent.click(screen.getByRole("button", { name: `Show diff for ${GONE}` }));

  expect((await screen.findByRole("group", { name: "Loaded diff" })).textContent).toBe(
    JSON.stringify({ before: "committed text", after: null }),
  );
  expect(store.get(checkpointAtom)?.hashes).toEqual({ [A]: "old", [SAME]: "same" });
});

it("a failed diff leaves the document unseen", async () => {
  failures[`/api/at-commit?commit=c1&path=${encodeURIComponent(A)}`] = {
    status: 500,
    message: "Cannot read old text",
  };
  const { store } = mount();
  await openList();

  fireEvent.click(screen.getByRole("button", { name: `Show diff for ${A}` }));

  expect(await screen.findByText("Cannot read old text")).toBeTruthy();
  expect(store.get(checkpointAtom)).toEqual(checkpoint);
});

it("closing the dialog returns focus to the control that opened it", async () => {
  mount();
  const opener = await screen.findByRole("button", { name: /Changed since you looked/ });
  fireEvent.click(opener);
  await screen.findByRole("button", { name: `Show diff for ${A}` });

  fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });

  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  await waitFor(() => expect(document.activeElement).toBe(opener));
});

it("closing after mark all seen puts focus on the page main, not the body", async () => {
  const main = document.createElement("main");
  document.body.append(main);
  mount();
  await openList();
  fireEvent.click(screen.getByRole("button", { name: "Mark all seen" }));
  await screen.findByText("No changed documents");

  fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });

  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  await waitFor(() => expect(document.activeElement).toBe(main));
  main.remove();
});
