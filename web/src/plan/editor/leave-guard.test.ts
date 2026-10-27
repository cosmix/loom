import { act, cleanup, render, waitFor } from "@testing-library/react";
import { createElement, useState } from "react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useLeaveGuard } from "@/plan/editor/leave-guard";

function Editor({ initiallyDirty }: { initiallyDirty: boolean }) {
  const [dirty, setDirty] = useState(initiallyDirty);
  useLeaveGuard(dirty);
  return createElement("button", { onClick: () => setDirty(false) }, "Mark clean");
}

function setup(dirty: boolean) {
  const router = createMemoryRouter(
    [
      { path: "/edit", element: createElement(Editor, { initiallyDirty: dirty }) },
      { path: "/other", element: createElement("div", null, "Other") },
    ],
    { initialEntries: ["/edit"] },
  );
  const view = render(createElement(RouterProvider, { router }));
  return { router, ...view };
}

function unload() {
  const event = new Event("beforeunload", { cancelable: true });
  window.dispatchEvent(event);
  return event.defaultPrevented;
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("useLeaveGuard", () => {
  it("allows clean navigation and browser exits without prompting", async () => {
    const confirm = vi.spyOn(window, "confirm");
    const { router } = setup(false);
    expect(unload()).toBe(false);
    await act(async () => {
      await router.navigate("/other");
    });
    expect(router.state.location.pathname).toBe("/other");
    expect(confirm).not.toHaveBeenCalled();
  });

  it("keeps the editor when navigation is declined and leaves when confirmed", async () => {
    const confirm = vi
      .spyOn(window, "confirm")
      .mockReturnValueOnce(false)
      .mockReturnValueOnce(true);
    const { router } = setup(true);
    await act(async () => {
      await router.navigate("/other");
    });
    await waitFor(() => expect(confirm).toHaveBeenCalledTimes(1));
    expect(router.state.location.pathname).toBe("/edit");
    await act(async () => {
      await router.navigate("/other");
    });
    await waitFor(() => expect(router.state.location.pathname).toBe("/other"));
    expect(confirm).toHaveBeenCalledTimes(2);
  });

  it("guards browser exits only while dirty and removes the listener on unmount", () => {
    const { getByText, unmount } = setup(true);
    expect(unload()).toBe(true);
    act(() => {
      getByText("Mark clean").click();
    });
    expect(unload()).toBe(false);
    unmount();
    expect(unload()).toBe(false);
  });

  it("removes the browser exit guard when unmounted while dirty", () => {
    const { unmount } = setup(true);
    expect(unload()).toBe(true);
    unmount();
    expect(unload()).toBe(false);
  });
});
