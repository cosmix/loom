import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { NamePrompt, useResendStoredName } from "@/plan/header/name-prompt";
import { planSocketAtom } from "@/plan/state/atoms";

const NAME_KEY = "loom-plan-name";

function fakeSocket() {
  const handlers = new Map<string, () => void>();
  const send = vi.fn();
  const subscribe = vi.fn((type: string, handler: () => void) => {
    handlers.set(type, handler);
    return () => handlers.delete(type);
  });
  const store = createStore();
  store.set(planSocketAtom, { subscribe, send } as never);
  return { store, send, subscribe, handlers };
}

function Resender() {
  useResendStoredName();
  return null;
}

beforeEach(() => window.localStorage.clear());
afterEach(cleanup);

describe("useResendStoredName", () => {
  it("re-sends the stored name on every hello", () => {
    window.localStorage.setItem(NAME_KEY, "Ada");
    const { store, send, subscribe, handlers } = fakeSocket();
    render(
      <Provider store={store}>
        <Resender />
      </Provider>,
    );
    expect(subscribe).toHaveBeenCalledWith("hello", expect.any(Function));
    handlers.get("hello")?.();
    handlers.get("hello")?.();
    expect(send).toHaveBeenCalledTimes(2);
    expect(send).toHaveBeenCalledWith({ type: "set_name", name: "Ada" });
  });

  it("sends nothing when no name is stored", () => {
    const { store, send, handlers } = fakeSocket();
    render(
      <Provider store={store}>
        <Resender />
      </Provider>,
    );
    handlers.get("hello")?.();
    expect(send).not.toHaveBeenCalled();
  });
});

describe("NamePrompt", () => {
  it("sends set_name and stores the trimmed name on save", () => {
    const { store, send } = fakeSocket();
    render(
      <Provider store={store}>
        <NamePrompt />
      </Provider>,
    );
    fireEvent.change(screen.getByLabelText("Your name"), { target: { value: "  Grace  " } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(send).toHaveBeenCalledWith({ type: "set_name", name: "Grace" });
    expect(window.localStorage.getItem(NAME_KEY)).toBe("Grace");
    expect(screen.queryByLabelText("Your name")).toBeNull();
  });

  it("caps the field at the 32 characters the server keeps", () => {
    const { store } = fakeSocket();
    render(
      <Provider store={store}>
        <NamePrompt />
      </Provider>,
    );
    expect(screen.getByLabelText("Your name").getAttribute("maxlength")).toBe("32");
  });
});
