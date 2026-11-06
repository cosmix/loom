import { act, cleanup, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import type { ChatSnapshot, Item, Turn } from "@/plan/api/schema-chat";
import { clearedChatAtom } from "./shown-session";
import { fixture, item, mount, turn } from "./transcript-test-kit";

type Session = NonNullable<ChatSnapshot["session"]>;

beforeEach(() => {
  // The starting brief and the opening label load the session brief; it never answers here.
  vi.stubGlobal(
    "fetch",
    vi.fn(() => new Promise<Response>(() => {})),
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const session = (state: Session["state"], fields: Partial<Session> = {}): Session => ({
  ...fixture.session!,
  state,
  failure: null,
  ...fields,
});
const said = (id: string): Item =>
  item("user", { id, text: "A message", origin: null, attachments: [], delivery: "delivered" });
const closingPrompt = item("user", {
  id: "closing",
  text: "Hidden closing prompt",
  origin: "closing",
  attachments: [],
  delivery: "delivered",
});
const edit = item("tool", {
  id: "e1",
  tool_use_id: "tu-e1",
  name: "Edit",
  status: "done",
  summary: "doc/product/demo/section_01.md",
  added: 4,
  removed: 1,
});
const changed = [{ path: "doc/product/demo/section_01.md", before_hash: "h", by_tool: true }];
const at = (minute: number) => `2026-11-05T10:0${minute}:00Z`;

it("a working chat ends its newest open turn with the typing bubble and no roundel", () => {
  const closed = turn([said("u1")], { id: "t1", open: false, started_at: at(1) });
  const open = turn([said("u2")], { id: "t2", open: true, started_at: at(2) });
  const { log } = mount([], { session: session("working"), turns: [closed, open] });

  const typing = log.querySelectorAll(".chat-typing");
  expect(typing).toHaveLength(1);
  expect(typing[0]!.querySelectorAll(".chat-typing-dot")).toHaveLength(3);
  expect(typing[0]!.textContent).toBe("Working");
  expect(typing[0]!.getAttribute("aria-live")).toBe("off");
  expect(typing[0]!.closest("section")?.getAttribute("aria-label")).toBe("Turn 2");
  expect(log.querySelector(".plan-spinner")).toBeNull();
});

it("a working pressure run draws no typing bubble in the transcript", () => {
  const open = turn([said("u1")], { open: true });
  const pressure = { ...session("working"), kind: "pressure" } as ChatSnapshot["session"];
  const { log } = mount([], { session: pressure, turns: [open] });

  expect(log.querySelector(".chat-typing")).toBeNull();
  expect(log.querySelector(".plan-spinner")).toBeNull();
});

it("no typing bubble while the chat is idle, or on a closing turn", () => {
  const open = turn([said("u1")], { open: true });
  const idle = mount([], { session: session("idle"), turns: [open] });
  expect(idle.log.querySelector(".chat-typing")).toBeNull();
  expect(idle.log.querySelector(".plan-spinner")).toBeNull();
  cleanup();

  const closing = turn([closingPrompt], { id: "t2", open: true, started_at: at(2) });
  const { log } = mount([], { session: session("ending"), turns: [closing] });
  expect(log.querySelector(".chat-typing")).toBeNull();
  expect(log.querySelectorAll(".plan-spinner")).toHaveLength(1);
});

it("an ending chat shows the wrap-up once the user has written, before the closing turn", () => {
  const talk = turn([said("u1")], { open: false });
  const { log } = mount([], { session: session("ending"), turns: [talk] });

  expect(log.querySelector(".chat-wrap")?.textContent).toContain("Writing down what we decided");
  expect(log.querySelectorAll(".chat-wrap-ticks")).toHaveLength(0);
});

it("an ending chat the user never wrote to shows no wrap-up", () => {
  const { log } = mount([], { session: session("ending"), turns: [] });

  expect(log.querySelector(".chat-wrap")).toBeNull();
});

it("an ended chat whose closing write never ran still shows its documents", () => {
  const talk = turn([said("u1"), edit], { open: false, changed_files: changed });
  const { log } = mount([], { session: session("ended"), turns: [talk] });

  const card = log.querySelector(".chat-left-behind")!;
  expect(card.textContent).toContain("section_01.md");
  expect(card.textContent).toContain("+4 −1");
});

it("an ended chat whose closing turn is still open draws the left-behind card, not the wrap-up", () => {
  const talk = turn([said("u1")], { id: "t1", open: false });
  const closing = turn([closingPrompt], { id: "t2", open: true, started_at: at(2) });
  const { log } = mount([], { session: session("ended"), turns: [talk, closing] });

  expect(log.querySelectorAll(".chat-left-behind")).toHaveLength(1);
  expect(log.querySelector(".chat-wrap")).toBeNull();
  expect(log.querySelector(".plan-spinner")).toBeNull();
  expect(log.textContent).not.toContain("Writing down what we decided");
});

it("an ended chat that changed nothing, or that the user never wrote to, shows no card", () => {
  const quiet = turn([said("u1")], { open: false });
  const first = mount([], { session: session("ended"), turns: [quiet] });
  expect(first.log.querySelector(".chat-left-behind")).toBeNull();
  cleanup();

  const unasked = turn([edit], { open: false, changed_files: changed });
  const second = mount([], { session: session("ended"), turns: [unasked] });
  expect(second.log.querySelector(".chat-left-behind")).toBeNull();
});

it("a starting chat shows the starting card with the pane's spinner", () => {
  const { log } = mount([], { session: session("starting"), turns: [] });

  const card = log.querySelector(".chat-starting")!;
  expect(card.querySelectorAll(".plan-spinner")).toHaveLength(1);
  expect(card.textContent).toContain("Starting");
});

it("a failed chat ends with the failure notice", () => {
  const failure = { kind: "died" as const, message: "The session process exited." };
  const { log } = mount([], { session: session("failed", { failure }), turns: [] });

  expect(within(log).getByText("The session stopped unexpectedly")).toBeTruthy();
  expect(log.textContent).toContain("The session process exited.");
});

const ended = (id: string, turns: Turn[]): NonNullable<ChatSnapshot["previous"]> => ({
  session: session("ended", { id }),
  turns,
});

it("a previous chat the user cleared with New chat is not drawn, nor its divider", () => {
  const old = turn([item("text", { id: "old", markdown: "Old chat reply" })], { open: false });
  const { log, store } = mount([], { session: null, turns: [], previous: ended("old-id", [old]) });
  expect(log.textContent).toContain("Old chat reply");

  act(() => store.set(clearedChatAtom, "old-id"));

  expect(log.textContent).not.toContain("Old chat reply");
  expect(log.textContent).not.toMatch(/Chat ended/);
});

it("a previous chat stays when a different chat was cleared", () => {
  const old = turn([item("text", { id: "old", markdown: "Old chat reply" })], { open: false });
  const { log, store } = mount([], { session: null, turns: [], previous: ended("old-id", [old]) });

  act(() => store.set(clearedChatAtom, "another-id"));

  expect(log.textContent).toContain("Old chat reply");
});

it("the current turns of a cleared concluded chat are not drawn, a live chat's always are", () => {
  const reply = item("text", { id: "now", markdown: "Current reply" });
  const concluded = mount([reply], { session: session("ended", { id: "s1" }) });
  act(() => concluded.store.set(clearedChatAtom, "s1"));
  expect(concluded.log.textContent).not.toContain("Current reply");
  cleanup();

  const live = mount([reply], { session: session("working", { id: "s1" }) });
  act(() => live.store.set(clearedChatAtom, "s1"));
  expect(live.log.textContent).toContain("Current reply");
});
