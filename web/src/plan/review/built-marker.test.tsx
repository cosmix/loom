import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import docFixture from "@/plan/api/fixtures/files-doc.json";
import { BuiltMarker } from "./built-marker";
import { hashesAtom } from "./live";

vi.mock("@/plan/diff/diff-view", () => ({
  DiffView: () => <div role="group" aria-label="Loaded diff" />,
}));

const section = "doc/product/section_01.md";
const builtUrl = "/api/built?section=product%2Fsection_01.md";
const notes = {
  notes: [{ kind: "changed_after", slug: "feat-review", built: "2026-10-17", base_commit: "base" }],
};
const linkName = "Changed after feat-review was built";
let builtCalls: number;
let gate: Promise<void> | null;

beforeEach(() => {
  builtCalls = 0;
  gate = null;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string) => {
      if (input === builtUrl) {
        builtCalls += 1;
        if (builtCalls > 1) await gate;
        return new Response(JSON.stringify(notes));
      }
      if (input.startsWith("/api/at-commit")) {
        return new Response(JSON.stringify({ path: section, text: "before" }));
      }
      return new Response(JSON.stringify({ ...docFixture, path: section, text: "after" }));
    }),
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function mount() {
  const store = createStore();
  store.set(hashesAtom, { head: "c1", docs: { [section]: "one" } });
  render(
    <Provider store={store}>
      <MemoryRouter>
        <BuiltMarker section={section} />
      </MemoryRouter>
    </Provider>,
  );
  return store;
}

it("closing the comparison returns focus to the marker link", async () => {
  mount();
  const marker = await screen.findByRole("link", { name: linkName });
  fireEvent.click(marker);
  await screen.findByRole("group", { name: "Loaded diff" });

  fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });

  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  await waitFor(() => expect(document.activeElement).toBe(marker));
});

it("a hashes change keeps the marker on show while the refetch is pending", async () => {
  const store = mount();
  await screen.findByRole("link", { name: linkName });
  let release: () => void = () => {};
  gate = new Promise<void>((resolve) => (release = resolve));

  act(() => store.set(hashesAtom, { head: "c1", docs: { [section]: "two" } }));

  await waitFor(() => expect(builtCalls).toBe(2));
  expect(screen.getByRole("link", { name: linkName })).toBeTruthy();
  expect(screen.queryByRole("status")).toBeNull();
  await act(async () => release());
  expect(screen.getByRole("link", { name: linkName })).toBeTruthy();
});

it("a refetch keeps an open comparison open", async () => {
  const store = mount();
  fireEvent.click(await screen.findByRole("link", { name: linkName }));
  await screen.findByRole("dialog");

  act(() => store.set(hashesAtom, { head: "c1", docs: { [section]: "two" } }));

  await waitFor(() => expect(builtCalls).toBe(2));
  expect(screen.getByRole("dialog")).toBeTruthy();
});
