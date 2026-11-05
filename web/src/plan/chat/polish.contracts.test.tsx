import { render, screen, within } from "@testing-library/react";
import { Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { expect, it, vi } from "vitest";

import { serverInfoAtom } from "@/plan/state/atoms";
import {
  installComposerLifecycle,
  mount as mountComposer,
  setup as setupComposer,
} from "./composer-harness";
import { ChatPanel, ChatRegistrations } from "./panel";
import { installPanelLifecycle, storeFor } from "./panel-test-kit";
import { storageKey } from "./storage";

// The panel's own body is under test; its children are stood in for, as panel.test.tsx does.
// The composer stays real, so its Message textbox shows whether the panel body is open.
vi.mock("./transcript", () => ({ Transcript: () => <div role="log">Transcript</div> }));
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
vi.mock("./model", async (original) => ({
  ...(await original<typeof import("./model")>()),
  useChatSocket: vi.fn(),
}));
vi.mock("@/plan/review/changed-since", () => ({
  ChangedSinceControl: () => <button aria-label="Changed since">Changed since · 2 files</button>,
}));

installPanelLifecycle();
installComposerLifecycle();

/** A wide desktop: every min-width breakpoint holds, whatever width the panel switches at. */
function wideViewport() {
  vi.spyOn(window, "matchMedia").mockImplementation((query) => ({
    matches: /min-width/.test(query) && !/max-width|prefers-reduced-motion/.test(query),
    media: query,
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: () => true,
  }));
}

it("the chat panel has no collapse at a wide viewport", () => {
  wideViewport();
  const store = storeFor();
  const key = store.get(serverInfoAtom)!.project_key;
  localStorage.setItem(storageKey(key, "panel-collapsed"), "true");
  render(
    <Provider store={store}>
      <MemoryRouter>
        <ChatRegistrations />
        <ChatPanel />
      </MemoryRouter>
    </Provider>,
  );
  const panel = screen.getByRole("complementary", { name: "Chat" });
  expect(within(panel).getByRole("textbox", { name: "Message" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: /Collapse chat|Expand chat/ })).toBeNull();
});

it("Stop sits in the composer's action row", () => {
  const { store } = setupComposer("working");
  mountComposer(store);
  const stop = screen.getByRole("button", { name: "Stop" });
  expect(stop.closest(".chat-composer-actions")).not.toBeNull();
});
