import { act, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";

import docFixture from "@/plan/api/fixtures/files-doc.json";
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

/** What /api/doc serves for A; each test moves it as A is edited. */
const doc = { text: "two", hash: "h2" };
const kitFetch = fetchMock.getMockImplementation()!;

beforeEach(() => {
  Object.assign(doc, { text: "two", hash: "h2" });
  hashesWithA("h2");
  fetchMock.mockImplementation(async (input: string) => {
    const url = new URL(input, "http://localhost");
    if (url.pathname === "/api/at-commit" && url.searchParams.get("commit") === "c1") {
      return new Response(JSON.stringify({ path: url.searchParams.get("path"), text: "one" }));
    }
    if (url.pathname === "/api/doc" && `doc/${url.searchParams.get("path")}` === A) {
      return new Response(JSON.stringify({ ...docFixture, path: A, ...doc }));
    }
    return kitFetch(input);
  });
});

function hashesWithA(hash: string) {
  replies["/api/hashes"] = { head: "c2", docs: { ...current.docs, [A]: hash } };
}

function atCommitRequests() {
  return fetchMock.mock.calls.filter(([input]) => {
    const url = new URL(input, "http://localhost");
    return url.pathname === "/api/at-commit" && url.searchParams.get("commit") === "c1";
  }).length;
}

const side = (name: "Before" | "After") => screen.getByRole("group", { name }).textContent;

async function openDiffOfA(after: string) {
  fireEvent.click(await screen.findByRole("button", { name: `Show diff for ${A}` }));
  await waitFor(() => expect(side("After")).toBe(after));
}

it("a viewed document's next diff starts from what was seen", async () => {
  const { emit } = mount({ live: true });
  await openList();
  await openDiffOfA("two");

  Object.assign(doc, { text: "three", hash: "h3" });
  hashesWithA("h3");
  act(() => emit({ type: "doc_changed", path: A, hash: "h3" }));

  await openDiffOfA("three");
  expect(side("Before")).toBe("two");
  expect(screen.queryByText("one")).toBeNull();
});

it("seen text outlives a missing checkpoint commit", async () => {
  const first = mount({ live: true });
  await openList();
  await openDiffOfA("two");
  const requestsBefore = atCommitRequests();
  cleanup();

  replies["/api/compare?commit=c1"] = { commit: "c1", available: false, docs: [] };
  Object.assign(doc, { text: "three", hash: "h3" });
  hashesWithA("h3");
  const second = mount({ live: true });
  // The kit names a fresh project per mount; a remount of the same page keeps the first one.
  act(() => second.store.set(serverInfoAtom, { ...server, project_key: first.projectKey }));
  await openList();

  await openDiffOfA("three");
  expect(side("Before")).toBe("two");
  expect(atCommitRequests()).toBe(requestsBefore);
});
