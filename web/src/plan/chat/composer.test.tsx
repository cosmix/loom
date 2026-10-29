import { act, fireEvent, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";

import helloFixture from "@/plan/api/fixtures/hello.json";
import { checklistViewSchema } from "@/plan/api/schema";
import { registerOfferAction } from "@/plan/offers/actions";
import { authoringAtom, checklistAtom, stoppingAtom } from "@/plan/state/atoms";
import { ComposerArea } from "./composer";
import { snapshotAtom, unconfirmedSendsAtom } from "./model";
import { StopControl } from "./stop-control";
import {
  box,
  installComposerLifecycle,
  mount,
  queue,
  send,
  setup,
  ShortcutHost,
  type,
} from "./composer-harness";

installComposerLifecycle();

it("viewer sees the read-only line, no textbox", () => {
  const { store } = setup("idle", true);
  mount(store);
  expect(screen.queryByRole("textbox", { name: "Message" })).toBeNull();
  expect(screen.getByText(/is authoring/).textContent).toContain("Read-only · Ada is authoring");
});
it("everyone composes when authoring is free", () => {
  const { store } = setup();
  store.set(authoringAtom, { holder: null });
  mount(store);
  type();
  expect(send().getAttribute("aria-disabled")).toBeNull();
});
it("send with no chat issues start", () => {
  const h = setup(null);
  mount(h.store);
  type();
  fireEvent.click(send());
  expect(h.sent[0].command).toBe("start");
});
it("send while starting queues", () => {
  const h = setup("starting");
  mount(h.store);
  type();
  fireEvent.click(send());
  expect(h.sent[0].command).toBe("send");
});
it("send while idle", async () => {
  const h = setup();
  mount(h.store);
  type();
  fireEvent.click(send());
  expect(h.sent[0]).toMatchObject({
    type: "chat",
    command: "send",
    text: "Hello",
    attachments: [],
  });
  expect(box().value).toBe("Hello");
  await h.result();
  expect(box().value).toBe("");
});
it("stop shown while working", () => {
  const h = setup("working");
  mount(h.store);
  expect(screen.getByRole("button", { name: "Stop" })).toBeTruthy();
});
it("stop issues interrupt", () => {
  const h = setup("working");
  mount(h.store);
  fireEvent.click(screen.getByRole("button", { name: "Stop" }));
  expect(h.sent[0].command).toBe("interrupt");
});
it("viewer has no stop", () => {
  const h = setup("working", true);
  mount(h.store);
  expect(screen.queryByRole("button", { name: "Stop" })).toBeNull();
});
it("ending disables with reason", () => {
  const h = setup("ending");
  mount(h.store);
  type();
  expect(send().getAttribute("aria-disabled")).toBe("true");
  expect(screen.getByText("the chat is closing")).toBeTruthy();
});
function offerPrimary(store: ReturnType<typeof setup>["store"]) {
  const primary = {
    ...checklistViewSchema.parse(helloFixture.checklists[0]).primary!,
    label: "Write section 05",
  };
  store.set(checklistAtom, {
    feature: "demo",
    current: { kind: "ready" },
    steps: [],
    primary,
    secondary: [],
    items: [],
  });
  return registerOfferAction(primary.kind, vi.fn());
}
it("ended replaces the composer with New chat and the primary offer", () => {
  const h = setup("ended");
  const unregister = offerPrimary(h.store);
  mount(h.store);
  expect(screen.queryByRole("textbox")).toBeNull();
  expect(screen.queryByRole("button", { name: "Send" })).toBeNull();
  expect(screen.getByRole("button", { name: "New chat" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Write section 05" })).toBeTruthy();
  unregister();
});
it("failed replaces the composer with New chat alone", () => {
  const h = setup("failed");
  const unregister = offerPrimary(h.store);
  mount(h.store);
  expect(screen.queryByRole("textbox")).toBeNull();
  expect(screen.queryByRole("button", { name: "Send" })).toBeNull();
  expect(screen.getByRole("button", { name: "New chat" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Write section 05" })).toBeNull();
  unregister();
});
it.each(["failed", "ended"] as const)(
  "a browser that connects after the chat %s gets New chat, not a live composer",
  (state) => {
    const h = setup(state);
    // The bridge moved the concluded chat to `previous`; a late browser receives no current one.
    h.store.set(snapshotAtom, (s) => ({
      ...s,
      session: null,
      previous: { session: { ...s.session!, state }, turns: [] },
    }));
    mount(h.store);
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(screen.getByRole("button", { name: "New chat" })).toBeTruthy();
  },
);
it("a viewer of an ended chat still sees the read-only line", () => {
  const { store } = setup("ended", true);
  mount(store);
  expect(screen.getByText(/is authoring/)).toBeTruthy();
  expect(screen.queryByRole("button", { name: "New chat" })).toBeNull();
});
it("stopping disables", () => {
  const h = setup();
  h.store.set(stoppingAtom, true);
  mount(h.store);
  type();
  fireEvent.click(send());
  expect(h.sent).toEqual([]);
  expect(screen.getByText("loom plan is stopping")).toBeTruthy();
});
it("enter sends, shift enter newline", () => {
  const h = setup();
  mount(h.store);
  type();
  fireEvent.keyDown(box(), { key: "Enter", shiftKey: true });
  expect(h.sent).toEqual([]);
  fireEvent.keyDown(box(), { key: "Enter" });
  expect(h.sent).toHaveLength(1);
});
it("grows to eight rows", () => {
  const h = setup();
  mount(h.store);
  type("a\nb\nc");
  expect(box().rows).toBe(3);
  type("line\n".repeat(20));
  expect(box().rows).toBe(8);
  expect(box().style.maxHeight).toBe("186px");
});
it("c focuses composer", () => {
  const h = setup();
  mount(
    h.store,
    <>
      <ShortcutHost />
      <ComposerArea />
    </>,
  );
  fireEvent.keyDown(document.body, { key: "c" });
  expect(document.activeElement).toBe(box());
  const input = document.createElement("input");
  document.body.append(input);
  input.focus();
  fireEvent.keyDown(input, { key: "c" });
  expect(document.activeElement).toBe(input);
  input.remove();
});
it("delivery states", async () => {
  const h = setup();
  queue(h.store);
  mount(h.store);
  expect(screen.getByText("Queued (3)")).toBeTruthy();
  type();
  fireEvent.click(send());
  expect(screen.getByText("Sending")).toBeTruthy();
  await h.result();
  expect(screen.queryByText("Sending")).toBeNull();
  expect(screen.queryByRole("button", { name: "Resend" })).toBeNull();
});
it("resend issues send again", async () => {
  const h = setup();
  h.store.set(unconfirmedSendsAtom, [{ client_id: "lost", text: "Old text", attachments: [] }]);
  mount(h.store);
  fireEvent.click(screen.getByRole("button", { name: "Resend" }));
  expect(h.sent[0]).toMatchObject({ command: "send", text: "Old text" });
  await h.result();
  expect(screen.queryByRole("button", { name: "Resend" })).toBeNull();
});
it("resending an old entry keeps a draft that holds the same text", async () => {
  const h = setup();
  h.store.set(unconfirmedSendsAtom, [{ client_id: "lost", text: "Hello", attachments: [] }]);
  mount(h.store);
  type("Hello");
  fireEvent.click(screen.getByRole("button", { name: "Resend" }));
  expect(h.sent[0]).toMatchObject({ command: "send", text: "Hello" });
  await h.result();
  expect(screen.queryByRole("button", { name: "Resend" })).toBeNull();
  expect(box().value).toBe("Hello");
});
it("queue strip lists first lines", () => {
  const h = setup();
  queue(h.store);
  mount(h.store);
  expect(screen.getByText("first 0")).toBeTruthy();
  expect(screen.queryByText("second 0")).toBeNull();
  expect(screen.getByText("+1 more")).toBeTruthy();
});
it("remove withdraws", () => {
  const h = setup();
  queue(h.store, 1);
  mount(h.store);
  fireEvent.click(screen.getByRole("button", { name: "Remove" }));
  expect(h.sent[0]).toMatchObject({ command: "withdraw", queue_id: "0" });
});
it("edit withdraws and refills composer", async () => {
  const h = setup();
  queue(h.store, 1);
  mount(h.store);
  type("Draft");
  fireEvent.click(screen.getByRole("button", { name: "Edit" }));
  expect(box().value).toBe("Draft");
  await h.result();
  expect(box().value).toBe("Draft\n\nfirst 0\nsecond 0");
});
it("viewer sees queue count only", () => {
  const h = setup("working", true);
  queue(h.store);
  mount(h.store);
  expect(screen.getByText("Queued (3)")).toBeTruthy();
  expect(screen.queryByText("first 0")).toBeNull();
  expect(screen.queryByRole("button", { name: "Edit" })).toBeNull();
});
it("escape in the panel does not interrupt", () => {
  const h = setup("working");
  mount(h.store);
  fireEvent.keyDown(box(), { key: "Escape" });
  expect(h.sent).toEqual([]);
});
it("refused command shows reason", async () => {
  const h = setup();
  mount(h.store);
  type();
  fireEvent.click(send());
  await h.result("The session refuses this message");
  expect(screen.getByText("The session refuses this message")).toBeTruthy();
});
it("first-hold note once", () => {
  const h = setup();
  const first = mount(h.store);
  expect(screen.getByText(/Closing this tab/)).toBeTruthy();
  first.unmount();
  mount(h.store);
  expect(screen.queryByText(/Closing this tab/)).toBeNull();
});
it("no beforeunload listener", () => {
  const listener = vi.spyOn(window, "addEventListener");
  const h = setup();
  mount(h.store);
  expect(listener.mock.calls.some(([event]) => event === "beforeunload")).toBe(false);
});
it("composerBlock reason disables send", () => {
  const h = setup("ending");
  mount(h.store);
  type();
  fireEvent.click(send());
  expect(h.sent).toEqual([]);
});
it("stop confirm prop asks first", () => {
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
  const h = setup("working");
  mount(h.store, <StopControl confirm="Stop this turn?" />);
  fireEvent.click(screen.getByRole("button", { name: "Stop" }));
  expect(confirm).toHaveBeenCalledWith("Stop this turn?");
  expect(h.sent).toEqual([]);
  confirm.mockReturnValue(true);
  fireEvent.click(screen.getByRole("button", { name: "Stop" }));
  expect(h.sent[0].command).toBe("interrupt");
});
const starting = (store: ReturnType<typeof setup>["store"]) =>
  act(() =>
    store.set(snapshotAtom, (s) => ({ ...s, session: { ...s.session!, state: "starting" } })),
  );

it.each(["ended", "failed"] as const)(
  "focus on New chat of a %s chat moves to the text box that replaces it",
  (state) => {
    const h = setup(state);
    mount(h.store);
    const newChat = screen.getByRole("button", { name: "New chat" });
    newChat.focus();
    expect(document.activeElement).toBe(newChat);
    starting(h.store);
    expect(screen.queryByRole("button", { name: "New chat" })).toBeNull();
    expect(document.activeElement).toBe(box());
  },
);
it("a chat that starts elsewhere leaves focus alone when it was not on New chat", () => {
  const h = setup("ended");
  mount(h.store);
  starting(h.store);
  expect(box()).toBeTruthy();
  expect(document.activeElement).toBe(document.body);
});
