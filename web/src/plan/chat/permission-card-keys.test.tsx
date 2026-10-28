import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { Provider } from "jotai/react";
import { expect, it } from "vitest";

import { youAtom } from "@/plan/state/atoms";
import { snapshotAtom } from "./model";
import { PermissionCard } from "./permission-card";
import {
  card,
  harness,
  payload,
  permission,
  type Permission,
  snapshot,
  useFakeClock,
} from "./permission-card-test-kit";

useFakeClock();

it("viewer sees the holder name and no buttons", () => {
  harness([permission()], true).show();

  expect(screen.getByText("Waiting for Dana to decide")).toBeTruthy();
  expect(within(card()).queryAllByRole("button")).toEqual([]);
});

it("y and n keys", async () => {
  const h = harness();
  h.show();
  card().focus();
  fireEvent.keyDown(card(), { key: "n" });
  expect(screen.getByRole("textbox", { name: "Deny note" })).toBe(document.activeElement);
  card().focus();
  fireEvent.keyDown(card(), { key: "y" });

  expect(h.send).toHaveBeenCalledWith(
    expect.objectContaining({ ...payload, allow: true, note: null }),
  );
  await act(async () => h.reply());
});

it("keys ignored in composer", () => {
  const h = harness();
  render(
    <Provider store={h.store}>
      <aside aria-label="Chat">
        <PermissionCard item={permission()} />
        <textarea aria-label="Message" />
      </aside>
    </Provider>,
  );
  const composer = screen.getByRole("textbox", { name: "Message" });
  composer.focus();
  fireEvent.keyDown(composer, { key: "y" });
  fireEvent.keyDown(composer, { key: "n" });

  expect(h.send).not.toHaveBeenCalled();
  expect(screen.queryByRole("textbox", { name: "Deny note" })).toBeNull();
});

it("keys ignored in deny note and inline copy", () => {
  const h = harness();
  const view = h.show();
  fireEvent.keyDown(card(), { key: "n" });
  fireEvent.keyDown(screen.getByRole("textbox", { name: "Deny note" }), { key: "y" });
  view.unmount();
  h.show(permission(), true);
  fireEvent.keyDown(card(), { key: "y" });

  expect(h.send).not.toHaveBeenCalled();
});

it.each<[Permission["status"], Permission["deny_reason"], string | null, string]>([
  ["allowed", null, null, "Allowed: Bash"],
  ["denied", "holder", "Keep it", "Denied: Bash · Keep it"],
  ["denied", "timeout", null, "Denied automatically after one hour: Bash"],
  ["denied", "interrupt", null, "Denied when the turn was stopped: Bash"],
  ["denied", "session_ended", null, "Denied: the session was ending"],
  ["denied", "server_stopping", null, "Denied: the session was ending"],
  ["denied", "hook_error", null, "Denied: the permission hook failed: Bash"],
])("outcome lines: %s %s", (status, deny_reason, note, expected) => {
  harness().show(permission({ status, deny_reason, note }));

  expect(card().textContent).toBe(expected);
  expect(within(card()).queryAllByRole("button")).toEqual([]);
  expect(card().querySelector("pre")).toBeNull();
});

it("assertive announcement and focus rule", () => {
  const h = harness();
  function Panel({ show }: { show: boolean }) {
    return (
      <Provider store={h.store}>
        <button>Outside</button>
        <aside aria-label="Chat">
          <textarea aria-label="Message" />
          {show && (
            <>
              <PermissionCard item={permission()} inline />
              <PermissionCard item={permission()} />
            </>
          )}
        </aside>
      </Provider>
    );
  }
  const view = render(<Panel show={false} />);
  const composer = screen.getByRole("textbox", { name: "Message" });
  composer.focus();
  view.rerender(<Panel show />);
  const pinned = screen.getAllByRole("region", { name: "Permission: Bash" })[1];

  // Focus stays in the editable composer, so a typed "y" never reaches the card.
  expect(document.activeElement).toBe(composer);
  fireEvent.keyDown(composer, { key: "y" });
  expect(h.send).not.toHaveBeenCalled();
  expect(document.querySelectorAll('[aria-live="assertive"]')).toHaveLength(1);
  expect(pinned.querySelector('[aria-live="assertive"]')?.textContent).toBe(
    "Needs a decision: Bash",
  );
  screen.getByRole("button", { name: "Outside" }).focus();
  view.rerender(<Panel show />);
  expect(document.activeElement).toBe(screen.getByRole("button", { name: "Outside" }));
  view.rerender(<Panel show={false} />);
  view.rerender(<Panel show />);
  expect(document.activeElement).toBe(screen.getByRole("button", { name: "Outside" }));
});

it("focus moves to the pinned card only from a non-editable control in the panel", () => {
  const h = harness();
  function Panel({ show }: { show: boolean }) {
    return (
      <Provider store={h.store}>
        <aside aria-label="Chat">
          <button>Send</button>
          {show && <PermissionCard item={permission()} />}
        </aside>
      </Provider>
    );
  }
  const view = render(<Panel show={false} />);
  screen.getByRole("button", { name: "Send" }).focus();
  view.rerender(<Panel show />);

  expect(document.activeElement).toBe(card());
});

it("1 of 2 decisions", () => {
  harness([permission(), permission({ id: "p2", request_id: "r2" })]).show();

  expect(screen.getByText("1 of 2 decisions")).toBeTruthy();
});

it("second pending permission pins after the first is answered", async () => {
  const first = permission(),
    second = permission({ id: "p2", request_id: "r2", tool: "Search" });
  const h = harness([first, second]);
  const view = h.show();
  fireEvent.click(screen.getByRole("button", { name: "Allow" }));
  await act(async () => h.reply());
  act(() =>
    h.store.set(snapshotAtom, {
      ...snapshot,
      turns: [{ ...snapshot.turns[0], items: [{ ...first, status: "allowed" }, second] }],
    }),
  );
  view.rerender(
    <Provider store={h.store}>
      <PermissionCard item={second} />
    </Provider>,
  );
  const next = screen.getByRole("region", { name: "Permission: Search" });
  next.focus();
  fireEvent.keyDown(next, { key: "y" });

  expect(screen.queryByText("1 of 2 decisions")).toBeNull();
  expect(h.send).toHaveBeenLastCalledWith(
    expect.objectContaining({ request_id: "r2", allow: true }),
  );
  await act(async () => h.reply());
});

it("refused answer stays pending and shows the reason", async () => {
  const h = harness();
  h.show();
  fireEvent.click(screen.getByRole("button", { name: "Allow" }));
  await act(async () => h.reply("Dana no longer holds authoring"));

  expect(screen.getByRole("alert").textContent).toBe("Dana no longer holds authoring");
  expect(screen.getByRole("button", { name: "Allow" }).hasAttribute("disabled")).toBe(false);
});

it("losing authoring removes the note actions and ignores keys", () => {
  const h = harness();
  h.show();
  fireEvent.keyDown(card(), { key: "n" });
  act(() => h.store.set(youAtom, { public_id: "reader", name: "Reader" }));
  fireEvent.keyDown(card(), { key: "y" });

  expect(screen.queryAllByRole("button")).toEqual([]);
  expect(screen.queryByRole("textbox", { name: "Deny note" })).toBeNull();
  expect(h.send).not.toHaveBeenCalled();
});
