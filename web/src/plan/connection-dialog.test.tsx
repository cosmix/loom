import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { afterEach, expect, it, vi } from "vitest";

import server from "@/plan/api/fixtures/server-info.json";
import { connectPlanSocket } from "@/plan/api/socket";
import { PlannerConnectionDialog } from "@/plan/connection-dialog";
import { connectionAtom, stoppingAtom, unauthorizedAtom } from "@/plan/state/atoms";

class FakeSocket {
  static instances: FakeSocket[] = [];
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  close() {
    this.onclose?.();
  }
  constructor() {
    FakeSocket.instances.push(this);
  }
}

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  FakeSocket.instances = [];
});

function mount() {
  const store = createStore();
  render(
    <Provider store={store}>
      <PlannerConnectionDialog />
    </Provider>,
  );
  return store;
}

async function flush() {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(0);
  });
}

it("waits for connection failure and leaves graceful shutdown to the stopping notice", () => {
  const store = mount();
  expect(screen.queryByRole("alertdialog")).toBeNull();
  act(() => {
    store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: null });
    store.set(stoppingAtom, true);
  });
  expect(screen.queryByRole("alertdialog")).toBeNull();
});

it("shows a hazard modal when the backend is down on arrival, then closes on recovery", async () => {
  vi.useFakeTimers();
  vi.stubGlobal("fetch", vi.fn().mockRejectedValueOnce(new TypeError("Failed to fetch")));
  const store = mount();
  const handle = connectPlanSocket(store, {
    WebSocketImpl: FakeSocket as unknown as typeof WebSocket,
  });
  await flush();
  const dialog = screen.getByRole("alertdialog", {
    name: "Loom feature planner is not responding",
  });
  expect(dialog.querySelector(".hazard-warning")).toBeTruthy();
  expect(dialog.textContent).toContain("run loom plan");
  expect(document.activeElement === dialog || dialog.contains(document.activeElement)).toBe(true);
  fireEvent.keyDown(dialog, { key: "Escape" });
  expect(screen.getByRole("alertdialog")).toBe(dialog);

  vi.mocked(fetch).mockResolvedValue(new Response(JSON.stringify(server)));
  await act(async () => {
    await vi.advanceTimersByTimeAsync(1000);
    FakeSocket.instances[0]!.onopen?.();
  });
  expect(screen.queryByRole("alertdialog")).toBeNull();

  act(() => FakeSocket.instances[0]!.close());
  expect(screen.getByRole("alertdialog")).toBeTruthy();
  handle.dispose();
});

it("defers to sign-in when a reconnect requires authorization", () => {
  const store = mount();
  act(() => store.set(connectionAtom, { phase: "reconnecting", everOpen: true, lastMessageAt: 1 }));
  expect(screen.getByRole("alertdialog")).toBeTruthy();
  act(() => store.set(unauthorizedAtom, true));
  expect(screen.queryByRole("alertdialog")).toBeNull();
});
