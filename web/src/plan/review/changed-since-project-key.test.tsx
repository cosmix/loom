import { act, render, screen, waitFor } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { expect, it } from "vitest";

import docFixture from "@/plan/api/fixtures/files-doc.json";
import { serverInfoAtom } from "@/plan/state/atoms";
import { useSelectedDiff } from "./changed-since-data";
import { A, fetchMock, installChangedSinceLifecycle, server } from "./changed-since-test-kit";

installChangedSinceLifecycle();

const kitFetch = fetchMock.getMockImplementation()!;

/**
 * Holds every `/api/doc` read of A until `release` runs; the nth read answers with the text
 * `read n`, so the test can tell which read the dialog rendered. The rest of the kit answers at once.
 */
function holdDocReads() {
  const waiting: Array<() => void> = [];
  let reads = 0;
  fetchMock.mockImplementation(async (input: string) => {
    const url = new URL(input, "http://localhost");
    if (url.pathname === "/api/doc" && `doc/${url.searchParams.get("path")}` === A) {
      const text = `read ${++reads}`;
      await new Promise<void>((resolve) => waiting.push(resolve));
      return new Response(JSON.stringify({ ...docFixture, path: A, text, hash: "new" }));
    }
    return kitFetch(input);
  });
  return {
    release: () => waiting.splice(0).forEach((resolve) => resolve()),
    /** Answers the nth read (1-based) alone, so a later read can finish before an earlier one. */
    releaseRead: (nth: number) => waiting[nth - 1]?.(),
    reads: () => reads,
  };
}

/** Renders the diff hook alone, on a page whose server info is `key` (null: not heard yet). */
function mountDiff(key: string | null) {
  const store = createStore();
  if (key) store.set(serverInfoAtom, { ...server, project_key: key });
  const ref = { commit: "c1", name: "main" };
  function Probe() {
    const { load } = useSelectedDiff({ path: A, ref });
    if (!load) return <p>Loading the diff</p>;
    return <p role="status">{"error" in load ? load.error : load.after}</p>;
  }
  render(
    <Provider store={store}>
      <Probe />
    </Provider>,
  );
  return store;
}

async function changeProjectWhileLoading(from: string | null, to: string) {
  const held = holdDocReads();
  const store = mountDiff(from);
  await waitFor(() => expect(held.reads()).toBe(1));
  expect(screen.getByText("Loading the diff")).toBeTruthy();
  act(() => store.set(serverInfoAtom, { ...server, project_key: to }));
  await waitFor(() => expect(held.reads()).toBe(2));
  return held;
}

it("the project key arriving while a diff loads still ends in the comparison", async () => {
  (await changeProjectWhileLoading(null, "arrived")).release();

  expect((await screen.findByRole("status")).textContent).toBe("read 2");
});

it("a response cut from another project never renders", async () => {
  const held = await changeProjectWhileLoading("first", "elsewhere");

  // The current project's read answers first; the cut read answers after it and must be dropped.
  held.releaseRead(2);
  expect((await screen.findByRole("status")).textContent).toBe("read 2");
  held.releaseRead(1);
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 20));
  });
  expect(screen.getByRole("status").textContent).toBe("read 2");
});
