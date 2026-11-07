import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, expect, it } from "vitest";

import fixture from "@/plan/api/fixtures/chat-snapshot.json";
import { chatSnapshotSchema } from "@/plan/api/schema-chat";
import { checklistAtom } from "@/plan/state/atoms";
import { checklist } from "./context-kit";
import { contextWish } from "./context-attach";
import { ContextLine } from "./context-line";
import { snapshotAtom } from "./model";
import { ScreenSync } from "./screen-sync";
import { contextDocumentAtom, NO_DOCUMENT } from "./viewed-document";

const SECTION_FIVE = "/f/atlas/product/section_05";

afterEach(cleanup);

function mount(path: string, state: "idle" | "ended" | null = null) {
  const store = createStore();
  store.set(checklistAtom, checklist());
  const session = state
    ? { ...chatSnapshotSchema.parse(fixture).session!, state, feature: "atlas" }
    : null;
  store.set(snapshotAtom, { session, turns: [], queue: [], previous: null });
  const router = createMemoryRouter(
    [
      {
        path: "*",
        element: (
          <>
            <ScreenSync />
            <ContextLine />
          </>
        ),
      },
    ],
    { initialEntries: [path] },
  );
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  return { store, router };
}

const line = (name: string) => screen.queryByLabelText(`Viewing ${name}`);
const go = (router: ReturnType<typeof mount>["router"], path: string) =>
  act(() => router.navigate(path));

it("reads About and the document's name, named Viewing for assistive technology", () => {
  mount(SECTION_FIVE);
  const group = screen.getByRole("group", { name: "Viewing section_05" });
  expect(group.textContent?.replace(/\s+/g, " ").trim()).toBe("About section_05");
  expect(screen.getByRole("button", { name: "Dismiss context" })).toBeTruthy();
});

it("reads Now viewing while a chat runs, and About again once it ended", () => {
  mount(SECTION_FIVE, "idle");
  expect(line("section_05")?.textContent).toContain("Now viewing section_05");
  cleanup();
  mount(SECTION_FIVE, "ended");
  expect(line("section_05")?.textContent).toContain("About section_05");
});

it("is not a live region, since it changes on every navigation", () => {
  mount(SECTION_FIVE, "idle");
  const group = line("section_05")!;
  expect(group.closest("[aria-live]")).toBeNull();
  expect(group.querySelector("[role=status], [role=alert], [aria-live]")).toBeNull();
});

it("follows navigation with nothing clicked, and is gone on a page with no document", async () => {
  const { router } = mount(SECTION_FIVE);
  await go(router, "/f/atlas/engineering");
  expect(line("section_05")).toBeNull();
  expect(line("Engineering overview")?.textContent).toContain("About Engineering overview");
  await go(router, "/new");
  expect(screen.queryByRole("group")).toBeNull();
});

it("dismissing hides the line for that page until the user moves to another one", async () => {
  const { router } = mount(SECTION_FIVE);
  fireEvent.click(screen.getByRole("button", { name: "Dismiss context" }));
  expect(line("section_05")).toBeNull();

  await go(router, "/f/atlas/product/section_07");
  expect(line("section_07")).not.toBeNull();
  await go(router, SECTION_FIVE);
  expect(line("section_05")).not.toBeNull();
});

it("a dismissal survives the checklist arriving or changing on the same page", () => {
  const { store } = mount(SECTION_FIVE);
  // The checklist has not arrived yet: the line names the page and has no source.
  act(() => store.set(checklistAtom, null));
  fireEvent.click(screen.getByRole("button", { name: "Dismiss context" }));
  act(() => store.set(checklistAtom, checklist()));
  expect(line("section_05")).toBeNull();
  expect(store.get(contextDocumentAtom)).toEqual(NO_DOCUMENT);
});

it("a dismissal follows the document to another address of the same file", async () => {
  const { store, router } = mount(SECTION_FIVE);
  fireEvent.click(screen.getByRole("button", { name: "Dismiss context" }));

  await go(router, "/files/product/atlas/section_05.md");

  expect(line("section_05.md")).toBeNull();
  expect(store.get(contextDocumentAtom)).toEqual(NO_DOCUMENT);
  expect(contextWish(store)).toBeNull();
});

it("a dismissal holds across pages that share one file, and ends at the next document", async () => {
  const { router } = mount("/f/atlas/product");
  fireEvent.click(screen.getByRole("button", { name: "Dismiss context" }));

  await go(router, "/f/atlas/vision");
  expect(line("vision")).toBeNull();

  await go(router, "/f/atlas/product/section_07");
  expect(line("section_07")).not.toBeNull();
});

it("a dismissal on a page without a source ends at any navigation", async () => {
  const { store, router } = mount("/f/atlas/product");
  // Without its checklist the overview names no source file.
  act(() => store.set(checklistAtom, null));
  fireEvent.click(screen.getByRole("button", { name: "Dismiss context" }));
  expect(line("Product overview")).toBeNull();

  await go(router, "/f/atlas/engineering");
  expect(line("Engineering overview")).not.toBeNull();
});

it("shows a name with an invisible control character spelled out", () => {
  // A file address needs no checklist to name its document; a section the checklist lacks has none.
  mount("/files/product/atlas/section_%E2%80%AE05.md");
  const group = screen.getByRole("group");
  expect(group.getAttribute("aria-label")).toBe("Viewing section_⟨U+202E⟩05.md");
  expect(group.textContent).toContain("section_⟨U+202E⟩05.md");
});
