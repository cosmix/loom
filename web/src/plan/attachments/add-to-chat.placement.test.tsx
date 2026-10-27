import { cleanup, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { authoringAtom, serverInfoAtom } from "@/plan/state/atoms";

import { AddToChat } from "./add-to-chat";
import type { AttachmentDraft } from "./types";

const draft: AttachmentDraft = {
  kind: "source",
  path: "doc/notes.md",
  anchor: "purpose",
  text: "words",
  lines: null,
  stageId: null,
  hash: "abc",
};

function setViewport(width: number, height: number) {
  Object.defineProperty(window, "innerWidth", { value: width, configurable: true, writable: true });
  Object.defineProperty(window, "innerHeight", {
    value: height,
    configurable: true,
    writable: true,
  });
}

// A free plan: anyone may author, so the Add to chat button shows.
function freePlan() {
  const store = createStore();
  store.set(serverInfoAtom, {
    project: "p",
    project_key: "p-1",
    port: 1,
    remote: false,
    read_only: [],
    stopping: false,
    you: { public_id: "aaaaaaaaaaaaaaaa", name: "Me" },
  });
  store.set(authoringAtom, { holder: null });
  return store;
}

function control(store: ReturnType<typeof createStore>, rect: DOMRect) {
  return (
    <Provider store={store}>
      <AddToChat draft={draft} rect={rect} onAdded={() => {}} />
    </Provider>
  );
}

function mount(rect: DOMRect) {
  return render(control(freePlan(), rect));
}

function groupStyle() {
  const group = screen.getByRole("group", { name: "Selection actions" });
  return { top: group.style.top, left: group.style.left };
}

beforeEach(() => {
  // jsdom lays nothing out: report the control as 100x30.
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 0, 100, 30));
  setViewport(320, 600);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  setViewport(1024, 768);
});

describe("AddToChat placement", () => {
  it("clamps the control inside the right edge of a narrow viewport", () => {
    mount(new DOMRect(250, 100, 60, 20));
    expect(groupStyle()).toEqual({ top: "126px", left: "212px" });
  });

  it("flips above a selection on the last visible line", () => {
    mount(new DOMRect(40, 560, 80, 20));
    expect(groupStyle()).toEqual({ top: "524px", left: "40px" });
  });

  it("renders nothing while the selection is outside the viewport", () => {
    mount(new DOMRect(40, 900, 80, 20));
    expect(screen.queryByRole("group")).toBeNull();
    expect(screen.queryByRole("button", { name: "Add to chat" })).toBeNull();
  });

  it("keeps the button reachable by name while visible", () => {
    mount(new DOMRect(40, 100, 80, 20));
    expect(screen.getByRole("button", { name: "Add to chat" })).toBeTruthy();
  });

  it("reappears at its place when a later rect is back inside the viewport", () => {
    const store = freePlan();
    const { rerender } = render(control(store, new DOMRect(40, 900, 80, 20)));
    expect(screen.queryByRole("group")).toBeNull();
    rerender(control(store, new DOMRect(40, 100, 80, 20)));
    expect(groupStyle()).toEqual({ top: "126px", left: "40px" });
  });
});
