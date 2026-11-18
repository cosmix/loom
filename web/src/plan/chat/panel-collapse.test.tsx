import { act, fireEvent, render, screen } from "@testing-library/react";
import { Provider, atom, useAtomValue } from "jotai";
import { MemoryRouter } from "react-router";
import { expect, it, vi } from "vitest";

import { DiffPage } from "@/plan/diff/diff-page";
import { ChangesPage } from "@/plan/review/changes-page";
import { serverInfoAtom } from "@/plan/state/atoms";
import { snapshotAtom } from "./model";
import {
  installPanelLifecycle,
  media,
  mount,
  permissionSnapshot,
  storeFor,
} from "./panel-test-kit";
import { chatCollapsedAtom, revealChat, useDiffViewCollapse } from "./sheet-state";
import { storageKey } from "./storage";

vi.mock("./transcript", () => ({ Transcript: () => <div role="log">Transcript</div> }));
vi.mock("./composer", () => ({ ComposerArea: () => <div>Composer</div> }));
vi.mock("./attention", () => ({ useAttention: vi.fn() }));
vi.mock("./run-header", () => ({ RunHeader: () => <div>Run header</div> }));
vi.mock("./start-chat", () => ({
  NewChatButton: () => <button>New chat</button>,
  StartingBrief: () => <div>Starting brief</div>,
}));
vi.mock("./suggestion-cards", async (original) => ({
  ...(await original<typeof import("./suggestion-cards")>()),
  useSuggestionFeed: vi.fn(),
}));
vi.mock("@/plan/review/live", async (original) => ({
  ...(await original<typeof import("@/plan/review/live")>()),
  useReviewLive: vi.fn(),
}));
vi.mock("./model", async (original) => ({
  ...(await original<typeof import("./model")>()),
  useChatSocket: vi.fn(),
}));

installPanelLifecycle();

const panel = () => document.getElementById("chat-panel")!;
const stored = (store: ReturnType<typeof storeFor>) =>
  localStorage.getItem(storageKey(store.get(serverInfoAtom)!.project_key, "panel-collapsed"));
const expandButton = () => screen.getByRole("button", { name: /^Expand chat/ });
const toggle = () => document.querySelector<HTMLElement>("[data-chat-sheet-trigger]")!;
const panelCollapse = () => panel().querySelector<HTMLElement>("[data-chat-collapse]")!;

function DiffProbe() {
  useDiffViewCollapse();
  return null;
}

/** Stands in for navigating to and from a diff screen. */
const onDiffAtom = atom(false);
function DiffRoute() {
  return useAtomValue(onDiffAtom) ? <DiffProbe /> : null;
}

it("the side panel starts expanded, and the header's Chat button shows it pressed", () => {
  mount();
  expect(panel().hidden).toBe(false);
  for (const collapse of [panelCollapse(), toggle()]) {
    expect(collapse.getAttribute("aria-label")).toBe("Collapse chat");
    expect(collapse.getAttribute("aria-expanded")).toBe("true");
    expect(collapse.getAttribute("aria-controls")).toBe("chat-panel");
  }
  expect(toggle().textContent).toContain("Chat");
});

it("the header's Chat button toggles the panel and keeps focus on itself", () => {
  const store = storeFor();
  mount(store);
  toggle().focus();
  fireEvent.click(toggle());
  expect(panel().hidden).toBe(true);
  expect(toggle().getAttribute("aria-label")).toBe("Expand chat");
  expect(toggle().getAttribute("aria-expanded")).toBe("false");
  expect(stored(store)).toBe("true");
  expect(document.activeElement).toBe(toggle());
  fireEvent.click(toggle());
  expect(panel().hidden).toBe(false);
  expect(toggle().getAttribute("aria-expanded")).toBe("true");
  expect(stored(store)).toBe("false");
});

it("Collapse chat hides the panel without unmounting it; the header's Chat button restores it", () => {
  const store = storeFor();
  store.set(snapshotAtom, permissionSnapshot());
  mount(store);
  const collapse = panelCollapse();
  collapse.focus();
  fireEvent.click(collapse);
  expect(panel().hidden).toBe(true);
  expect(screen.queryByRole("complementary", { name: "Chat" })).toBeNull();
  expect(panel().querySelector("[role=log]")).not.toBeNull();
  expect(stored(store)).toBe("true");
  // One pair of live regions, outside the hidden panel, still speaks for the chat.
  const alerts = document.querySelectorAll('[data-chat-announcer="alert"]');
  expect(alerts).toHaveLength(1);
  expect(panel().contains(alerts[0])).toBe(false);
  const expand = screen.getByRole("button", { name: "Expand chat, 1 decision waiting" });
  expect(expand.getAttribute("aria-expanded")).toBe("false");
  expect(expand.getAttribute("aria-haspopup")).toBeNull();
  expect(document.activeElement).toBe(expand);
  fireEvent.click(expand);
  expect(panel().hidden).toBe(false);
  expect(stored(store)).toBe("false");
  expect(document.activeElement).toBe(expand);
});

it("expanding with focus on the page body moves focus to the panel's Collapse chat", () => {
  const { store } = mount();
  act(() => store.set(chatCollapsedAtom, true));
  (document.activeElement as HTMLElement | null)?.blur();
  act(() => store.set(chatCollapsedAtom, false));
  expect(document.activeElement).toBe(panelCollapse());
});

it("a stored collapsed choice starts collapsed at desktop width and is ignored by the sheet", () => {
  const store = storeFor();
  localStorage.setItem(
    storageKey(store.get(serverInfoAtom)!.project_key, "panel-collapsed"),
    "true",
  );
  const { unmount } = mount(store);
  expect(panel().hidden).toBe(true);
  unmount();
  media(false);
  mount(store);
  fireEvent.click(screen.getByRole("button", { name: "Open chat" }));
  expect(screen.getByRole("dialog", { name: "Chat" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Collapse chat" })).toBeNull();
});

it("the panel collapses and expands in memory when storage throws", () => {
  const denied = () => {
    throw new DOMException("blocked", "SecurityError");
  };
  vi.spyOn(Storage.prototype, "getItem").mockImplementation(denied);
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(denied);
  mount();
  expect(panel().hidden).toBe(false);
  fireEvent.click(panelCollapse());
  expect(panel().hidden).toBe(true);
  fireEvent.click(expandButton());
  expect(panel().hidden).toBe(false);
});

it("a diff view collapses the panel once per entry and leaving restores the user's choice", () => {
  const { store } = mount(storeFor(), <DiffRoute />);
  const view = (diff: boolean) => act(() => store.set(onDiffAtom, diff));
  view(true);
  expect(panel().hidden).toBe(true);
  expect(stored(store)).toBeNull();
  view(false);
  expect(panel().hidden).toBe(false);
  view(true);
  fireEvent.click(expandButton());
  view(true);
  expect(panel().hidden).toBe(false);
  view(false);
  expect(panel().hidden).toBe(false);
  view(true);
  expect(panel().hidden).toBe(true);
});

it("revealChat and the c key expand a collapsed side panel", () => {
  const { store } = mount();
  act(() => store.set(chatCollapsedAtom, true));
  act(() => revealChat(store));
  expect(panel().hidden).toBe(false);
  act(() => store.set(chatCollapsedAtom, true));
  fireEvent.keyDown(document.body, { key: "c" });
  expect(panel().hidden).toBe(false);
});

it.each([
  ["the file diff page", <DiffPage path="doc/product/section_01.md" kind="open" />],
  ["the changes page", <ChangesPage />],
])("%s collapses the side panel", (_name, page) => {
  const store = storeFor();
  render(
    <Provider store={store}>
      <MemoryRouter>{page}</MemoryRouter>
    </Provider>,
  );
  expect(store.get(chatCollapsedAtom)).toBe(true);
});
