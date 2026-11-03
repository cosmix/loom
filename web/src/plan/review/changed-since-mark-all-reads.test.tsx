import { fireEvent, screen, waitFor, act } from "@testing-library/react";
import { expect, it } from "vitest";

import docFixture from "@/plan/api/fixtures/files-doc.json";
import { checkpointAtom } from "./checkpoint";
import {
  A,
  ADDED,
  current,
  fetchMock,
  installChangedSinceLifecycle,
  mount,
  openList,
  replies,
} from "./changed-since-test-kit";
import { hashesAtom } from "./live";
import { seenTextsAtom } from "./seen-text";

installChangedSinceLifecycle();

const kitFetch = fetchMock.getMockImplementation()!;

/** Serves /api/doc through `serve`, counting the reads in flight; other requests use the kit. */
function serveDocs(serve: (path: string) => { text: string; hash: string }) {
  const stats = { inFlight: 0, peak: 0, total: 0 };
  fetchMock.mockImplementation(async (input: string) => {
    const url = new URL(input, "http://localhost");
    if (url.pathname !== "/api/doc") return kitFetch(input);
    const path = `doc/${url.searchParams.get("path")}`;
    stats.total += 1;
    stats.inFlight += 1;
    stats.peak = Math.max(stats.peak, stats.inFlight);
    await new Promise((resolve) => setTimeout(resolve, 5));
    stats.inFlight -= 1;
    return new Response(JSON.stringify({ ...docFixture, path, ...serve(path) }));
  });
  return stats;
}

/** A checkpoint and live hashes over `count` documents (A first), every one changed. */
function manyDocs(count: number) {
  const paths = [A, ...Array.from({ length: count - 1 }, (_, i) => `doc/product/many-${i}.md`)];
  const old = Object.fromEntries(paths.map((p) => [p, "old"]));
  const live = { head: "c2", docs: Object.fromEntries(paths.map((p) => [p, "new"])) };
  replies["/api/hashes"] = live;
  return { paths, live, checkpoint: { commit: "c1", hashes: old } };
}

async function markAllWith(count: number) {
  const many = manyDocs(count);
  const { store } = mount({ checkpoint: many.checkpoint });
  act(() => store.set(hashesAtom, many.live));
  await openList();
  fireEvent.click(screen.getByRole("button", { name: "Mark all seen" }));
  return { store, ...many };
}

it("Mark all seen keeps at most four document reads in flight", async () => {
  const stats = serveDocs(() => ({ text: "text", hash: "new" }));
  const { store, paths } = await markAllWith(12);

  await waitFor(() => expect(Object.keys(store.get(seenTextsAtom))).toHaveLength(paths.length));

  expect(stats.total).toBe(paths.length);
  expect(stats.peak).toBeGreaterThan(1);
  expect(stats.peak).toBeLessThanOrEqual(4);
});

it("Mark all seen stops reading once the stored text passes the total cap", async () => {
  // 100k chars is 200 KB of storage, under the per-document cap; about ten fill the 2 MiB total.
  const stats = serveDocs(() => ({ text: "x".repeat(100_000), hash: "new" }));
  const { store } = await markAllWith(40);

  await waitFor(() => expect(store.get(checkpointAtom)?.commit).toBe("c2"));
  await waitFor(() => expect(stats.total).toBeGreaterThan(0));
  await new Promise((resolve) => setTimeout(resolve, 150));

  expect(stats.total).toBeGreaterThan(10);
  expect(stats.total).toBeLessThan(20);
});

it("Mark all seen keeps reading past texts too large to store", async () => {
  // 150k chars is 300 KB, over the per-document cap: stored nowhere, so they never fill the total.
  const stats = serveDocs(() => ({ text: "x".repeat(150_000), hash: "new" }));
  const { store, paths } = await markAllWith(20);

  await waitFor(() => expect(store.get(checkpointAtom)?.commit).toBe("c2"));
  await waitFor(() => expect(stats.total).toBe(paths.length));
  await new Promise((resolve) => setTimeout(resolve, 60));

  expect(stats.total).toBe(paths.length);
  expect(store.get(seenTextsAtom)).toEqual({});
});

it("a read at a hash other than the checkpoint's deletes the dead seen text", async () => {
  serveDocs((path) => ({ text: "moved on", hash: path === A ? "different" : "added" }));
  replies["/api/hashes"] = current;
  const { store } = mount();
  act(() => store.set(seenTextsAtom, { [A]: { hash: "stale", text: "old text", seen: 1 } }));
  await openList();

  fireEvent.click(screen.getByRole("button", { name: "Mark all seen" }));

  await waitFor(() => expect(store.get(seenTextsAtom)[ADDED]?.text).toBe("moved on"));
  expect(store.get(seenTextsAtom)[A]).toBeUndefined();
});
