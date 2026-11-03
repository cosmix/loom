import { act, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { createStore, getDefaultStore } from "jotai";
import { beforeEach, expect, it, vi } from "vitest";

import docFixture from "@/plan/api/fixtures/files-doc.json";
import { storageFullAtom, storageKey } from "@/plan/chat/storage";
import { serverInfoAtom } from "@/plan/state/atoms";
import {
  A,
  current,
  fetchMock,
  installChangedSinceLifecycle,
  mount,
  openList,
  replies,
  server,
} from "./changed-since-test-kit";
import { checkpointAtom } from "./checkpoint";
import { hashesAtom } from "./live";
import { DOC_LIMIT, TOTAL_LIMIT, recordSeen, seenTextsAtom } from "./seen-text";

vi.mock("@/plan/diff/diff-view", () => ({
  DiffView: ({ before, after }: { before: string | null; after: string | null }) => (
    <div role="group" aria-label="Loaded diff">
      <div role="group" aria-label="Before">
        {before ?? ""}
      </div>
      <div role="group" aria-label="After">
        {after ?? ""}
      </div>
    </div>
  ),
}));

installChangedSinceLifecycle();

type DocBody = { text: string; hash: string } | "gone";
/** What /api/doc serves for A, unless `holding` queues the request until it is released. */
let docOfA: DocBody = { text: "current text", hash: "new" };
let holding = false;
const held: Array<(body: DocBody) => void> = [];
const kitFetch = fetchMock.getMockImplementation()!;

const reply = (body: DocBody) =>
  body === "gone"
    ? new Response("{}", { status: 404 })
    : new Response(JSON.stringify({ ...docFixture, path: A, ...body }));

beforeEach(() => {
  docOfA = { text: "current text", hash: "new" };
  holding = false;
  held.length = 0;
  fetchMock.mockImplementation(async (input: string) => {
    const url = new URL(input, "http://localhost");
    if (url.pathname !== "/api/doc" || `doc/${url.searchParams.get("path")}` !== A) {
      return kitFetch(input);
    }
    if (!holding) return reply(docOfA);
    return new Promise<Response>((resolve) => held.push((body) => resolve(reply(body))));
  });
});

const side = (name: "Before" | "After") => screen.getByRole("group", { name }).textContent;

async function showDiffOfA() {
  fireEvent.click(await screen.findByRole("button", { name: `Show diff for ${A}` }));
}

function setA(store: ReturnType<typeof createStore>, hash: string | null) {
  const docs: Record<string, string> = { ...current.docs };
  if (hash === null) delete docs[A];
  else docs[A] = hash;
  act(() => store.set(hashesAtom, { head: "c2", docs }));
}

it("mark all seen during a diff fetch keeps the newer baseline", async () => {
  const { store } = mount();
  await openList();
  holding = true;
  await showDiffOfA();
  await waitFor(() => expect(held).toHaveLength(1));
  holding = false;
  docOfA = { text: "marked text", hash: "new" };

  fireEvent.click(screen.getByRole("button", { name: "Mark all seen" }));
  await waitFor(() => expect(store.get(seenTextsAtom)[A]?.text).toBe("marked text"));
  act(() => held[0]({ text: "old snapshot", hash: "older" }));
  await waitFor(() => expect(side("After")).toBe("old snapshot"));

  expect(store.get(seenTextsAtom)[A]).toMatchObject({ hash: "new", text: "marked text" });
  expect(store.get(checkpointAtom)).toMatchObject({ commit: "c2", hashes: { [A]: "new" } });
});

it("seen text survives a reload", async () => {
  const first = mount();
  await openList();
  await showDiffOfA();
  await waitFor(() => expect(side("After")).toBe("current text"));
  cleanup();

  replies["/api/hashes"] = { head: "c2", docs: { ...current.docs, [A]: "edited" } };
  docOfA = { text: "edited text", hash: "edited" };
  const second = mount({ live: true });
  // A fresh store is a reload; the kit names a fresh project per mount, so restore the first.
  act(() => second.store.set(serverInfoAtom, { ...server, project_key: first.projectKey }));
  await openList();
  await showDiffOfA();

  await waitFor(() => expect(side("After")).toBe("edited text"));
  expect(side("Before")).toBe("current text");
});

it("of two updates to one document resolving out of order, the newer is kept", async () => {
  const { store } = mount();
  await openList();
  await showDiffOfA();
  await waitFor(() => expect(side("After")).toBe("current text"));
  holding = true;

  setA(store, "h3");
  await waitFor(() => expect(held).toHaveLength(1));
  setA(store, "h4");
  await waitFor(() => expect(held).toHaveLength(2));
  act(() => held[1]({ text: "four", hash: "h4" }));
  await waitFor(() => expect(side("After")).toBe("four"));
  act(() => held[0]({ text: "three", hash: "h3" }));
  await new Promise((resolve) => setTimeout(resolve, 0));

  expect(side("After")).toBe("four");
  expect(side("Before")).toBe("current text");
  expect(store.get(seenTextsAtom)[A]).toMatchObject({ hash: "h4", text: "four" });
  expect(store.get(checkpointAtom)?.hashes[A]).toBe("h4");
});

it("a document deleted then recreated drops its old seen text", async () => {
  const { store } = mount();
  await openList();
  await showDiffOfA();
  await waitFor(() => expect(store.get(seenTextsAtom)[A]?.text).toBe("current text"));

  docOfA = "gone";
  setA(store, null);
  await waitFor(() => expect(store.get(seenTextsAtom)[A]).toBeUndefined());
  expect(Object.hasOwn(store.get(checkpointAtom)!.hashes, A)).toBe(false);

  docOfA = { text: "reborn", hash: "reborn" };
  setA(store, "reborn");
  await waitFor(() => expect(side("After")).toBe("reborn"));
  expect(side("Before")).toBe("committed text");
});

it("a storage that throws on read and write falls back to the commit", async () => {
  const getItem = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
    throw new Error("storage is blocked");
  });
  const setItem = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new Error("storage is blocked");
  });
  try {
    const { store } = mount();
    await openList();
    await showDiffOfA();

    await waitFor(() => expect(side("Before")).toBe("committed text"));
    expect(store.get(checkpointAtom)?.hashes[A]).toBe("new");
    expect(getDefaultStore().get(storageFullAtom)).toBe(false);
  } finally {
    getItem.mockRestore();
    setItem.mockRestore();
  }
});

it("an over-cap text is not kept, and the least recently seen go first", () => {
  const store = createStore();
  store.set(serverInfoAtom, { ...server, project_key: "seen-text-caps" });
  let now = 0;
  const clock = vi.spyOn(Date, "now").mockImplementation(() => ++now);
  try {
    // Each text costs 200,000 bytes, so ten fit in the total cap and the eleventh evicts one.
    const text = "x".repeat(100_000);
    for (let n = 0; n < 11; n++) recordSeen(store, { [`doc/${n}.md`]: { hash: `${n}`, text } });
    const kept = Object.keys(store.get(seenTextsAtom));
    expect(kept).toHaveLength(Math.floor(TOTAL_LIMIT / 200_000));
    expect(kept).not.toContain("doc/0.md");
    expect(kept).toContain("doc/10.md");

    recordSeen(store, { "doc/10.md": { hash: "big", text: "x".repeat(DOC_LIMIT / 2 + 1) } });
    expect(store.get(seenTextsAtom)["doc/10.md"]).toBeUndefined();
  } finally {
    clock.mockRestore();
  }
});

it.each([
  ["an older text", { text: "current text", hash: "new" }],
  ["a failure", "gone"],
] as const)(
  "a slow Mark all seen read returning %s keeps a newer acknowledgment",
  async (_, body: DocBody) => {
    replies["/api/compare?commit=c2"] = { commit: "c2", available: true, docs: [] };
    const { store } = mount();
    await openList();
    holding = true;
    fireEvent.click(screen.getByRole("button", { name: "Mark all seen" }));
    await waitFor(() => expect(held).toHaveLength(1));
    holding = false;

    docOfA = { text: "edited text", hash: "h3" };
    setA(store, "h3");
    await showDiffOfA();
    await waitFor(() => expect(store.get(seenTextsAtom)[A]?.hash).toBe("h3"));
    await act(async () => {
      held[0](body);
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(store.get(seenTextsAtom)[A]).toMatchObject({ hash: "h3", text: "edited text" });
    expect(store.get(checkpointAtom)?.hashes[A]).toBe("h3");
  },
);

it("the total cap covers every project's seen texts and counts the stored JSON", () => {
  const other = storageKey("seen-text-other", "seen-text");
  const text = "x".repeat(100_000);
  const older = Object.fromEntries(
    [0, 1, 2, 3, 4, 5].map((n) => [`doc/${n}.md`, { hash: `${n}`, text, seen: n }]),
  );
  localStorage.setItem(other, JSON.stringify(older));
  const store = createStore();
  store.set(serverInfoAtom, { ...server, project_key: "seen-text-origin" });
  let now = 100;
  const clock = vi.spyOn(Date, "now").mockImplementation(() => ++now);
  try {
    // About 200 KB an entry: ten fit, so each write past four evicts the other project's oldest.
    for (let n = 0; n < 6; n++) recordSeen(store, { [`doc/${n}.md`]: { hash: `${n}`, text } });
    expect(Object.keys(JSON.parse(localStorage.getItem(other)!))).toEqual([
      "doc/2.md",
      "doc/3.md",
      "doc/4.md",
      "doc/5.md",
    ]);
    for (let n = 6; n < 10; n++) recordSeen(store, { [`doc/${n}.md`]: { hash: `${n}`, text } });
    expect(localStorage.getItem(other)).toBeNull();
    expect(Object.keys(store.get(seenTextsAtom))).toHaveLength(10);

    // A quote is stored escaped, so 60,000 of them cost 240 KB: eight fit where nine would not.
    const quotes = '"'.repeat(60_000);
    for (let n = 0; n < 9; n++) recordSeen(store, { [`doc/${n}.md`]: { hash: "q", text: quotes } });
    const kept = store.get(seenTextsAtom);
    expect(Object.values(kept).filter((entry) => entry.hash === "q")).toHaveLength(8);
    expect(kept["doc/0.md"]).toBeUndefined();
    expect(getDefaultStore().get(storageFullAtom)).toBe(false);
  } finally {
    clock.mockRestore();
  }
});
