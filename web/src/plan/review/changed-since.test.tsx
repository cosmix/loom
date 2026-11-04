import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { useState, type ReactNode } from "react";
import { expect, it, vi } from "vitest";

import { storageKey } from "@/plan/chat/storage";
import { openDiff } from "@/plan/diff/open-diff";
import { Popover, PopoverContent, PopoverTrigger } from "@/plan/header/popover";
import { serverInfoAtom } from "@/plan/state/atoms";
import { changesFrom } from "./changed-since";
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
  expect(screen.getByRole("button", { name: /Changed since you looked · 3/ })).toBeTruthy();
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
  // The open document stays listed as the current row, now seen; the count drops.
  const row = within(screen.getByRole("list")).getByText(A).closest("li");
  expect(row?.getAttribute("aria-current")).toBe("true");
  expect(row?.textContent).toContain("seen");
  expect(screen.getByRole("button", { name: "Changed since you looked · 2" })).toBeTruthy();
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

it("a row without a seen copy names the checkpoint commit it diffs from", async () => {
  mount();
  const list = await openList();

  expect(within(list).getByText(A).closest("li")?.textContent).toContain("Changes since commit c1");
});

it("unavailable commit offers only uncommitted and ref diffs", async () => {
  replies["/api/compare?commit=c1"] = { commit: "c1", available: false, docs: [] };
  mount();

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

  const note = screen.getByText(/kept in this browser/).textContent;
  expect(note).toMatch(/browser.*origin.*port/);
  expect(note).toMatch(/checkpoint commit.*uncommitted edits/);
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

/** The activity popover's shape: non-modal, open, the trigger in its content. */
function InPopover({ children }: { children: ReactNode }) {
  const [open, setOpen] = useState(true);
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger>Activity</PopoverTrigger>
      <PopoverContent aria-label="In progress">{children}</PopoverContent>
    </Popover>
  );
}

const pageHeading = () =>
  screen.findByRole("heading", { level: 1, name: "Changed since you looked" });

it("the row shows the title and count and opens the page with focus on its heading", async () => {
  mount({ path: "/f/demo/product" });
  expect(screen.queryByRole("heading", { level: 1 })).toBeNull();

  const opener = await screen.findByRole("button", { name: "Changed since you looked · 3" });
  expect(opener.textContent).toContain("Changed since you looked");
  expect(opener.textContent).toContain("3");
  expect(opener.classList.contains("activity-row")).toBe(true);
  fireEvent.click(opener);

  const heading = await pageHeading();
  await waitFor(() => expect(document.activeElement).toBe(heading));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(await screen.findByRole("list", { name: "Changed documents" })).toBeTruthy();
});

it("opening the page from the popover closes it without returning focus to its trigger", async () => {
  mount({ path: "/f/demo/product", wrap: (trigger) => <InPopover>{trigger}</InPopover> });
  fireEvent.click(await screen.findByRole("button", { name: /Changed since you looked/ }));

  const heading = await pageHeading();
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "In progress" })).toBeNull());
  // Radix returns focus on close after a timeout; let it pass before checking where focus is.
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(document.activeElement).toBe(heading);
});

it("after mark all seen the page says so and leads back to where the reader came from", async () => {
  mount({ path: "/f/demo/product?view=source#s2" });
  fireEvent.click(await screen.findByRole("button", { name: /Changed since you looked/ }));
  await openList();
  const markAll = screen.getByRole("button", { name: "Mark all seen" });
  markAll.focus();

  fireEvent.click(markAll);

  expect(await screen.findByText("Nothing changed since you looked.")).toBeTruthy();
  expect(screen.queryByRole("list", { name: "Changed documents" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Mark all seen" })).toBeNull();
  const back = screen.getByRole("link", { name: "Back to where you were" });
  expect(back.getAttribute("href")).toBe("/f/demo/product?view=source#s2");
  // The emptied list took the focused button with it; the way back holds the focus instead.
  await waitFor(() => expect(document.activeElement).toBe(back));
});

it("a page reached directly with nothing changed leads Home and keeps focus on its heading", async () => {
  mount({ checkpoint: { commit: "c2", hashes: { ...current.docs } } });

  expect(await screen.findByText("Nothing changed since you looked.")).toBeTruthy();
  expect(screen.getByRole("link", { name: "Go to Home" }).getAttribute("href")).toBe("/");
  expect(document.activeElement).toBe(await pageHeading());
});

it("reopening from the page keeps the way back to the first page", async () => {
  mount({ path: "/plans/demo" });
  fireEvent.click(await screen.findByRole("button", { name: /Changed since you looked/ }));
  await openList();
  fireEvent.click(screen.getByRole("button", { name: /Changed since you looked/ }));
  await openList();

  fireEvent.click(screen.getByRole("button", { name: "Mark all seen" }));

  const back = await screen.findByRole("link", { name: "Back to where you were" });
  expect(back.getAttribute("href")).toBe("/plans/demo");
});

it("changesFrom takes only an in-app path", () => {
  expect(changesFrom({ from: "/f/demo/product" })).toBe("/f/demo/product");
  expect(changesFrom({ from: "//host" })).toBeNull();
  expect(changesFrom({ from: "/\\host" })).toBeNull();
  expect(changesFrom({ from: 7 })).toBeNull();
  expect(changesFrom(null)).toBeNull();
});
