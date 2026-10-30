import { act, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";

import fixture from "@/plan/api/fixtures/chat-snapshot.json";
import { chatSnapshotSchema } from "@/plan/api/schema-chat";
import { snapshotAtom } from "./model";
import chatCss from "./chat.css?raw";
import {
  installPanelLifecycle,
  media,
  mount,
  permissionSnapshot,
  pointerSetup,
  storeFor,
} from "./panel-test-kit";
import { panelWidthAtom } from "./prefs";

vi.mock("./transcript", () => ({ Transcript: () => <div role="log">Transcript</div> }));
vi.mock("./composer", () => ({ ComposerArea: () => <div>Composer</div> }));
vi.mock("./attention", () => ({ useAttention: vi.fn() }));
vi.mock("./run-header", () => ({ RunHeader: () => <div>Run header</div> }));
vi.mock("./start-chat", () => ({
  NewChatButton: () => <button>New chat</button>,
  StartingBrief: () => <div>Starting brief</div>,
}));
vi.mock("./end-chat", () => ({
  EndChatMenuItem: () => <button>End chat</button>,
  useCanEndChat: vi.fn(() => true),
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
  ChangedSinceControl: ({ compact }: { compact: boolean }) => (
    <button aria-label="Changed since">{compact ? "● 2" : "Changed since · 2 files"}</button>
  ),
}));

installPanelLifecycle();

const announced = (kind: "alert" | "status") =>
  document.querySelector(`[data-chat-announcer="${kind}"]`)?.textContent;

const alerts = () => document.querySelectorAll('[data-chat-announcer="alert"]');
const statuses = () => document.querySelectorAll('[data-chat-announcer="status"]');
const openSheet = () => fireEvent.click(screen.getByRole("button", { name: /^Open chat/ }));

function concluded(state: "idle" | "ended") {
  const { session } = chatSnapshotSchema.parse(fixture);
  return { session: { ...session!, state, failure: null }, turns: [], queue: [], previous: null };
}

it("a new permission request is announced with the panel collapsed or the sheet closed", async () => {
  const store = storeFor();
  store.set(snapshotAtom, { ...permissionSnapshot(), session: null, turns: [] });
  mount(store);
  fireEvent.click(screen.getByRole("button", { name: "Collapse chat" }));
  expect(screen.queryByRole("log")).toBeNull();
  act(() => store.set(snapshotAtom, permissionSnapshot()));
  await waitFor(() => expect(announced("alert")).toBe("Needs a decision: Bash"));
  cleanup();

  media(false);
  const closed = storeFor();
  mount(closed);
  expect(screen.queryByRole("dialog")).toBeNull();
  act(() => closed.set(snapshotAtom, permissionSnapshot()));
  await waitFor(() => expect(announced("alert")).toBe("Needs a decision: Bash"));
});

it("with the modal sheet open the announcements live inside it, where they are not aria-hidden", async () => {
  media(false);
  const store = storeFor();
  store.set(snapshotAtom, concluded("idle"));
  mount(store);
  openSheet();
  const dialog = screen.getByRole("dialog", { name: "Chat" });
  act(() => store.set(snapshotAtom, permissionSnapshot()));
  await waitFor(() => expect(announced("alert")).toBe("Needs a decision: Bash"));
  expect(alerts()).toHaveLength(1);
  expect(dialog.contains(alerts()[0])).toBe(true);
  expect(alerts()[0].closest("[aria-hidden='true']")).toBeNull();
  act(() => store.set(snapshotAtom, concluded("ended")));
  expect(statuses()).toHaveLength(1);
  expect(announced("status")).toBe("Chat ended");
  expect(statuses()[0].closest("[aria-hidden='true']")).toBeNull();
});

it("moving the announcements between the page and the sheet repeats and drops nothing", async () => {
  media(false);
  const store = storeFor();
  store.set(snapshotAtom, concluded("idle"));
  mount(store);
  openSheet();
  act(() => store.set(snapshotAtom, permissionSnapshot()));
  await waitFor(() => expect(announced("alert")).toBe("Needs a decision: Bash"));
  // Read at once, with no pause: a repeated announcement empties the region before it speaks.
  fireEvent.click(screen.getByRole("button", { name: "Close chat" }));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(alerts()).toHaveLength(1);
  expect(announced("alert")).toBe("Needs a decision: Bash");
  expect(alerts()[0].closest("[aria-hidden='true']")).toBeNull();
  openSheet();
  expect(alerts()).toHaveLength(1);
  expect(announced("alert")).toBe("Needs a decision: Bash");
  expect(screen.getByRole("dialog").contains(alerts()[0])).toBe(true);
});

it("a failed or ended chat is announced with the panel collapsed", () => {
  const store = storeFor();
  mount(store);
  fireEvent.click(screen.getByRole("button", { name: "Collapse chat" }));
  const { session } = chatSnapshotSchema.parse(fixture);
  const set = (state: "failed" | "ended") =>
    act(() =>
      store.set(snapshotAtom, {
        session: { ...session!, state, failure: { kind: "died", message: "gone" } },
        turns: [],
        queue: [],
        previous: null,
      }),
    );
  set("failed");
  expect(announced("status")).toBe("The session stopped unexpectedly");
  set("ended");
  expect(announced("status")).toBe("Chat ended");
});

it("the resize handle announces whole rem while a drag stores fractions", () => {
  pointerSetup();
  const { store } = mount();
  const separator = screen.getByRole("separator", { name: "Resize chat" });
  Object.assign(separator, {
    setPointerCapture: vi.fn(),
    hasPointerCapture: () => true,
    releasePointerCapture: vi.fn(),
  });
  fireEvent.pointerDown(separator, { button: 0, clientX: 500 });
  fireEvent.pointerMove(separator, { clientX: 490 });
  const width = store.get(panelWidthAtom);
  expect(Number.isInteger(width)).toBe(false);
  expect(separator.getAttribute("aria-valuenow")).toBe(String(Math.round(width)));
  expect(separator.getAttribute("aria-valuetext")).toBe(`${Math.round(width)} rem`);
});

it("the chat menu offers New chat while a chat runs, and leaves it to the composer once it concluded", () => {
  const store = storeFor();
  const { session } = chatSnapshotSchema.parse(fixture);
  const show = (state: "idle" | "failed" | "ended") =>
    act(() =>
      store.set(snapshotAtom, {
        session: { ...session!, state, failure: null },
        turns: [],
        queue: [],
        previous: null,
      }),
    );
  mount(store);
  show("idle");
  // The menu is a closed <details>: the item is in the document, a click away.
  expect(screen.getByLabelText("Chat menu").parentElement?.textContent).toContain("New chat");
  show("failed");
  expect(screen.queryByText("New chat")).toBeNull();
  show("ended");
  expect(screen.queryByText("New chat")).toBeNull();
});

it("a stale transcript carries no coloured mark and no fade", () => {
  // The banner and the "Last update" line say it is stale. A bar, shadow or border would be a
  // coloured spine (section 08, Surface); opacity on muted text falls below 4.5:1 (WCAG 1.4.3).
  expect(chatCss).not.toMatch(/chat-dim/);
  expect(chatCss).not.toMatch(/box-shadow|opacity|--tone-/);
});
