import { act, fireEvent, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";

import { PanelHead } from "./panel-head";
import { harness, installStartChatLifecycle, session } from "./start-chat-test-kit";

// The real End chat row inside the real head: the stub in panel.test.tsx cannot show whether the
// menu is still open when the row draws an error or a reason.
installStartChatLifecycle();

function openMenu(state: "idle" | "ending" = "idle") {
  const h = harness({ ...session, state });
  h.mount(<PanelHead sheet={false} />);
  const summary = screen.getByLabelText("Chat menu");
  const menu = summary.closest("details")!;
  menu.open = true;
  return { h, menu, summary };
}

it("a rejected End chat leaves the menu open with the error in the document", async () => {
  const { h, menu } = openMenu();
  fireEvent.click(screen.getByRole("button", { name: "End chat" }));
  expect(h.send).toHaveBeenCalledWith(expect.objectContaining({ command: "end" }));
  h.result(0, "The session did not answer");
  await act(async () => {});
  expect(menu.open).toBe(true);
  expect(menu.contains(screen.getByText("The session did not answer"))).toBe(true);
});

it("a resolved End chat closes the menu", async () => {
  const { h, menu } = openMenu();
  fireEvent.click(screen.getByRole("button", { name: "End chat" }));
  expect(menu.open).toBe(true);
  h.result();
  await act(async () => {});
  expect(menu.open).toBe(false);
});

it("a declined confirmation leaves the menu open and sends nothing", () => {
  const { h, menu } = openMenu();
  vi.mocked(window.confirm).mockReturnValueOnce(false);
  fireEvent.click(screen.getByRole("button", { name: "End chat" }));
  expect(h.send).not.toHaveBeenCalled();
  expect(menu.open).toBe(true);
});

it("a blocked End chat click leaves the menu open with its reason shown", () => {
  const { h, menu } = openMenu("ending");
  fireEvent.click(screen.getByRole("button", { name: "End chat" }));
  expect(h.send).not.toHaveBeenCalled();
  expect(menu.open).toBe(true);
  expect(menu.contains(screen.getByText("Writing final notes into the documents"))).toBe(true);
});

it("Escape outside the menu is left to its owner, Escape inside closes it", () => {
  const { menu, summary } = openMenu();
  const outside = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
  const stop = vi.spyOn(outside, "stopPropagation");
  document.body.dispatchEvent(outside);
  expect(menu.open).toBe(true);
  expect(stop).not.toHaveBeenCalled();
  fireEvent.keyDown(screen.getByRole("button", { name: "End chat" }), { key: "Escape" });
  expect(menu.open).toBe(false);
  expect(document.activeElement).toBe(summary);
});

it("focus leaving the menu closes it, focus moving within it does not", () => {
  const { menu, summary } = openMenu();
  const end = screen.getByRole("button", { name: "End chat" });
  fireEvent.focusOut(summary, { relatedTarget: end });
  expect(menu.open).toBe(true);
  // The window losing focus reports no related target and must not close the menu.
  fireEvent.focusOut(end, { relatedTarget: null });
  expect(menu.open).toBe(true);
  fireEvent.focusOut(end, { relatedTarget: document.body });
  expect(menu.open).toBe(false);
});
