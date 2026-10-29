import { cleanup, render } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import type { ReactNode } from "react";
import { afterEach, beforeEach, vi } from "vitest";

import fixture from "@/plan/api/fixtures/chat-snapshot.json";
import hello from "@/plan/api/fixtures/hello.json";
import { chatSnapshotSchema } from "@/plan/api/schema-chat";
import { authoringAtom, connectionAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";
import { ChatPanel, ChatRegistrations, ChatSheetButton } from "./panel";

// The test files own their `vi.mock` calls (vitest hoists them per file); this kit holds the rest of the setup.
let serial = 0;

export function media(desktop: boolean) {
  vi.spyOn(window, "matchMedia").mockImplementation((query) => ({
    matches: query === "(min-width: 1024px)" ? desktop : false,
    media: query,
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: () => true,
  }));
}

export function storeFor() {
  const store = createStore();
  store.set(serverInfoAtom, { ...hello.server, project_key: `panel-${++serial}`, read_only: [] });
  store.set(youAtom, hello.you);
  store.set(authoringAtom, { holder: { ...hello.you, away_since: null, frees_at: null } });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: Date.now() });
  return store;
}

export function mount(store = storeFor(), extra?: ReactNode) {
  const view = render(
    <Provider store={store}>
      <ChatRegistrations />
      <ChatSheetButton />
      <ChatPanel />
      {extra}
    </Provider>,
  );
  return { store, ...view };
}

export function permissionSnapshot(count = 1) {
  const snapshot = chatSnapshotSchema.parse(fixture);
  const permission = snapshot.turns
    .flatMap((turn) => turn.items)
    .find((item) => item.type === "permission");
  if (!permission) throw new Error("Fixture needs a permission");
  snapshot.turns[0].items = Array.from({ length: count }, (_, index) => ({
    ...permission,
    id: `permission-${index}`,
    request_id: `request-${index}`,
    tool: index === 0 ? "Bash" : "Edit",
    status: "pending" as const,
  }));
  return snapshot;
}

export function pointerSetup() {
  class TestPointerEvent extends MouseEvent {
    pointerId = 1;
  }
  vi.stubGlobal("PointerEvent", TestPointerEvent);
  document.documentElement.style.fontSize = "16px";
}

/** Registers the per-test reset; call once at the top of each panel test file. */
export function installPanelLifecycle() {
  beforeEach(() => {
    media(true);
    vi.clearAllMocks();
  });
  afterEach(() => {
    cleanup();
    localStorage.clear();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });
}
