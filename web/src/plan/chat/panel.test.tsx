import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { MemoryRouter } from "react-router";
import { expect, it, vi } from "vitest";

import roundelCss from "@/aurora-ui/feedback/busy-roundel.css?raw";
import fixture from "@/plan/api/fixtures/chat-snapshot.json";
import hello from "@/plan/api/fixtures/hello.json";
import { offerViewSchema } from "@/plan/api/schema";
import { chatSnapshotSchema } from "@/plan/api/schema-chat";
import {
  AddToChat,
  pendingAttachmentsAtom,
  selectionSecondaries,
  type AttachmentDraft,
} from "@/plan/attachments";
import { HeaderExtras } from "@/plan/header-extras";
import { offerAction } from "@/plan/offers/actions";
import { OfferButton } from "@/plan/offers/offer-button";
import { connectionAtom, serverInfoAtom, stoppingAtom, youAtom } from "@/plan/state/atoms";
import { useReviewLive } from "@/plan/review/live";
import { startChat } from "./commands";
import { snapshotAtom, useChatSocket } from "./model";
import {
  installPanelLifecycle,
  media,
  mount,
  permissionSnapshot,
  pointerSetup,
  storeFor,
} from "./panel-test-kit";
import { useSuggestionFeed } from "./suggestion-cards";
import chatCss from "./chat.css?raw";
import { panelWidthAtom } from "./prefs";
import { chatSheetOpenAtom } from "./sheet-state";
import { clearedChatAtom } from "./shown-session";
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
vi.mock("./commands", async (original) => ({
  ...(await original<typeof import("./commands")>()),
  startChat: vi.fn().mockResolvedValue({}),
}));
vi.mock("./model", async (original) => ({
  ...(await original<typeof import("./model")>()),
  useChatSocket: vi.fn(),
}));
vi.mock("@/plan/review/changed-since", () => ({
  ChangedSinceControl: () => <button aria-label="Changed since">Changed since · 2 files</button>,
}));

type Store = ReturnType<typeof createStore>;
const offerView = offerViewSchema.parse(hello.checklists[0]!.primary);
const draft: AttachmentDraft = {
  kind: "source",
  path: "doc/product/section_10.md",
  anchor: null,
  text: "Selected words",
  lines: null,
  stageId: null,
  hash: "abc",
};
const rect = { top: 10, left: 10, right: 60, bottom: 30, width: 50, height: 20 } as DOMRect;

installPanelLifecycle();

it("drag clamps to 20 and 40rem and persists", () => {
  pointerSetup();
  const { store } = mount();
  const separator = screen.getByRole("separator", { name: "Resize chat" });
  Object.assign(separator, {
    setPointerCapture: vi.fn(),
    hasPointerCapture: () => true,
    releasePointerCapture: vi.fn(),
  });
  fireEvent.pointerDown(separator, { button: 0, clientX: 500 });
  fireEvent.pointerMove(separator, { clientX: 5000 });
  expect(store.get(panelWidthAtom)).toBe(20);
  fireEvent.pointerMove(separator, { clientX: -5000 });
  fireEvent.pointerUp(separator, { clientX: -5000 });
  expect(store.get(panelWidthAtom)).toBe(40);
  expect(
    localStorage.getItem(storageKey(store.get(serverInfoAtom)!.project_key, "panel-width")),
  ).toBe("40");
});

it("separator arrow keys resize", () => {
  const { store } = mount();
  const separator = screen.getByRole("separator", { name: "Resize chat" });
  fireEvent.keyDown(separator, { key: "ArrowLeft" });
  expect(store.get(panelWidthAtom)).toBe(22.25);
  expect(separator.getAttribute("aria-valuenow")).toBe("22");
  fireEvent.keyDown(separator, { key: "ArrowRight" });
  expect(store.get(panelWidthAtom)).toBe(21.25);
  act(() => store.set(panelWidthAtom, 40));
  fireEvent.keyDown(separator, { key: "ArrowLeft" });
  expect(store.get(panelWidthAtom)).toBe(40);
});

it("New chat after a pressure run draws the empty headline alone, with no pressure head over it", () => {
  const store = storeFor();
  const { session } = chatSnapshotSchema.parse(fixture);
  const run = { ...session!, kind: "pressure", state: "ended" as const, failure: null };
  store.set(snapshotAtom, { session: run, turns: [], queue: [], previous: null });
  store.set(clearedChatAtom, run.id);
  mount(store);
  const panel = screen.getByRole("complementary", { name: "Chat" });
  expect(panel.querySelector(".chat-empty")).not.toBeNull();
  expect(within(panel).queryByText("Run header")).toBeNull();
});

it("sheet below lg", () => {
  media(false);
  const { store } = mount();
  expect(screen.queryByRole("complementary", { name: "Chat" })).toBeNull();
  // The hook runs on every render; the socket must be wired to this store while the sheet is closed.
  expect(useChatSocket).toHaveBeenCalled();
  for (const [used] of vi.mocked(useChatSocket).mock.calls) expect(used).toBe(store);
  fireEvent.click(screen.getByRole("button", { name: "Open chat" }));
  expect(screen.getByRole("dialog", { name: "Chat" })).toBeTruthy();
  expect(screen.getByRole("complementary", { name: "Chat" })).toBeTruthy();
  expect(screen.queryByRole("separator")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Close chat" }));
  expect(store.get(chatSheetOpenAtom)).toBe(false);
});

it("the header button names no pending attachment and draws no count for them", () => {
  media(false);
  const store = storeFor();
  store.set(pendingAttachmentsAtom, [draft]);
  mount(store);
  expect(store.get(chatSheetOpenAtom)).toBe(false);
  const button = screen.getByRole("button", { name: "Open chat" });
  expect(button.querySelector(".chat-count-badge")).toBeNull();
  expect(button.textContent).toBe("Chat");
  expect(screen.queryByRole("dialog")).toBeNull();
});

it("adding a chip opens the chat sheet and holds the attachment", () => {
  media(false);
  const { store } = mount(storeFor(), <AddToChat draft={draft} rect={rect} onAdded={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Add to chat" }));
  expect(store.get(pendingAttachmentsAtom)).toHaveLength(1);
  expect(store.get(chatSheetOpenAtom)).toBe(true);
  expect(screen.getByRole("dialog", { name: "Chat" })).toBeTruthy();
});

it("reduced motion stops the roundel", () => {
  // The roundel's own stylesheet owns this; the panel adds no second rule for it.
  expect(roundelCss).toMatch(
    /@media\s*\(prefers-reduced-motion:\s*reduce\)[\s\S]*\.aurora-roundel[\s\S]*animation:\s*none/,
  );
});

it("the panel is as tall as its row, and chat-column.css alone shares that height out", () => {
  expect(chatCss).toMatch(/\.chat-panel \{[^}]*\n\s*height: 100%;/);
  expect(chatCss).not.toMatch(/grid-template-rows|flex:/);
});

function disconnect(store: Store) {
  act(() =>
    store.set(connectionAtom, {
      phase: "reconnecting",
      everOpen: true,
      lastMessageAt: Date.now() - 30_000,
    }),
  );
}

it("lost banner", () => {
  const { store } = mount();
  disconnect(store);
  expect(screen.getByText("Connection lost · reconnecting")).toBeTruthy();
});

it("holder line", () => {
  const { store } = mount();
  disconnect(store);
  expect(screen.getByText(/disconnected for 10 minutes/)).toBeTruthy();
  act(() => store.set(youAtom, { public_id: "viewer", name: "Reader" }));
  expect(screen.queryByText(/disconnected for 10 minutes/)).toBeNull();
});

it("stopping", () => {
  const { store } = mount();
  act(() => store.set(stoppingAtom, true));
  expect(screen.getByText("loom plan is stopping. The session is writing its notes.")).toBeTruthy();
});

it("a stale transcript is marked by its Last update line, with no dimming class", () => {
  const live = storeFor();
  live.set(snapshotAtom, permissionSnapshot());
  const { store } = mount(live);
  disconnect(store);
  const wrapper = screen.getByRole("log").parentElement!;
  expect(screen.getByText(/Last update 30s ago/)).toBeTruthy();
  expect(wrapper.className).not.toContain("dim");
  act(() =>
    store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: Date.now() }),
  );
  expect(screen.queryByText(/Last update/)).toBeNull();
});

it("two pending permissions name both tools and count 2", () => {
  const store = storeFor();
  store.set(snapshotAtom, permissionSnapshot(2));
  mount(store);
  expect(screen.getByText("Needs your answer").closest("header")).toBeTruthy();
  cleanup();
  media(false);
  mount(store);
  expect(
    screen.getByRole("button", { name: "Open chat, 2 decisions waiting" }).textContent,
  ).toContain("2");
});

it.each([
  ["/f/demo", true],
  ["/", false],
  ["/new", false],
])("the header's chat button at %s below the desktop width: %s", (path, shown) => {
  media(false);
  render(
    <Provider store={storeFor()}>
      <MemoryRouter initialEntries={[path]}>
        <HeaderExtras />
      </MemoryRouter>
    </Provider>,
  );
  expect(screen.queryByRole("button", { name: "Open chat" }) !== null).toBe(shown);
  expect(screen.queryByRole("button", { name: "Changed since" })).toBeNull();
});

it("the header keeps its chat button while the side panel is expanded", () => {
  media(true);
  render(
    <Provider store={storeFor()}>
      <MemoryRouter initialEntries={["/f/demo"]}>
        <HeaderExtras />
      </MemoryRouter>
    </Provider>,
  );
  const button = screen.getByRole("button", { name: /^Collapse chat/ });
  expect(button.getAttribute("aria-expanded")).toBe("true");
});

it("below lg with the sheet closed, an OfferButton is a button", async () => {
  media(false);
  const { store, unmount } = mount();
  // Checklist data mounts after the shell's registration effect.
  const offer = render(
    <Provider store={store}>
      <OfferButton offer={offerView} />
    </Provider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Write section 05" }));
  expect(startChat).toHaveBeenCalledWith(
    store,
    "write-section:05",
    undefined,
    undefined,
    offerView.feature,
    undefined,
  );
  await act(async () => {});
  expect(store.get(chatSheetOpenAtom)).toBe(true);
  offer.unmount();
  unmount();
  expect(offerAction("write_section")).toBeUndefined();
});

it("below lg with the sheet closed, a viewer's selection shows Send to author", () => {
  media(false);
  const store = storeFor();
  store.set(youAtom, { public_id: "reader", name: "Reader" });
  const { unmount } = mount(store);
  const selection = render(
    <Provider store={store}>
      <AddToChat draft={draft} rect={rect} onAdded={() => {}} />
    </Provider>,
  );
  expect(screen.getByRole("button", { name: "Send to author" })).toBeTruthy();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(store.get(chatSheetOpenAtom)).toBe(false);
  selection.unmount();
  unmount();
  expect(selectionSecondaries()).toHaveLength(0);
  expect(useReviewLive).toHaveBeenCalled();
  expect(useSuggestionFeed).toHaveBeenCalled();
});

it("a refused offer start shows a dismissible alert inside the panel until the next success", async () => {
  vi.mocked(startChat).mockRejectedValueOnce(new Error("Another browser holds authoring (Alex)"));
  const { store } = mount();
  render(
    <Provider store={store}>
      <OfferButton offer={offerView} />
    </Provider>,
  );
  const start = async () => {
    fireEvent.click(screen.getByRole("button", { name: "Write section 05" }));
    await act(async () => {});
  };
  await start();
  const alert = within(screen.getByRole("complementary", { name: "Chat" })).getByRole("alert");
  expect(alert.textContent).toContain("Ada started a chat first");
  fireEvent.click(within(alert).getByRole("button", { name: "Dismiss" }));
  expect(screen.queryByRole("alert")).toBeNull();
  vi.mocked(startChat).mockRejectedValueOnce(new Error("Chat already running"));
  await start();
  expect(screen.getByRole("alert")).toBeTruthy();
  await start();
  expect(screen.queryByRole("alert")).toBeNull();
});

it("with no chat the panel shows the empty state in place of the transcript", () => {
  mount();
  expect(screen.queryByRole("log")).toBeNull();
  const empty = screen.getByRole("region", { name: "Let's spec!" });
  expect(empty.className).toBe("chat-empty");
  expect(empty.textContent).toContain("Your first message starts the session.");
  cleanup();
  const store = storeFor();
  store.set(snapshotAtom, permissionSnapshot());
  mount(store);
  expect(screen.getByRole("log")).toBeTruthy();
  expect(screen.queryByRole("region", { name: "Let's spec!" })).toBeNull();
});

it("the panel has no Chat menu and no End chat button, with or without a chat", () => {
  mount();
  expect(screen.queryByLabelText("Chat menu")).toBeNull();
  cleanup();
  const store = storeFor();
  store.set(snapshotAtom, permissionSnapshot());
  mount(store);
  expect(screen.queryByLabelText("Chat menu")).toBeNull();
  expect(screen.queryByRole("button", { name: "End chat", hidden: true })).toBeNull();
});

it("the sheet button's name carries the session state", () => {
  media(false);
  const store = storeFor();
  const { session } = chatSnapshotSchema.parse(fixture);
  const working = { ...session!, state: "working" as const };
  store.set(snapshotAtom, { session: working, turns: [], queue: [], previous: null });
  mount(store);
  expect(screen.getByRole("button", { name: "Open chat, Working" })).toBeTruthy();
});
