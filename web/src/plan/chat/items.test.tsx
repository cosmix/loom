import { cleanup, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { item, mount } from "./transcript-test-kit";

beforeEach(() => {
  // The opening-message label loads the session brief; it never answers here.
  vi.stubGlobal(
    "fetch",
    vi.fn(() => new Promise<Response>(() => {})),
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("a thinking marker draws nothing, whatever its duration", () => {
  for (const duration_ms of [null, 0, 1500]) {
    const { log, unmount } = mount([item("thinking", { id: "t", duration_ms })]);
    expect(log.querySelector(".chat-turn-items")?.textContent).toBe("");
    unmount();
  }
});

it("a user message is a bubble drawn by its classes, with no utility alignment", () => {
  const { log } = mount([item("user", { text: "ok", origin: null, attachments: [] })]);
  const words = within(log).getByText("ok");
  const body = words.closest(".chat-user-body")!;
  expect(body.parentElement?.className).toContain("chat-user");
  expect(body.className).toBe("chat-user-body");
  expect(body.parentElement?.className).not.toMatch(/w-fit|ml-auto/);
});

it("the opening message is one quiet line, never a bubble, with its label inline", () => {
  const { log } = mount([item("user", { text: "Brief text", origin: "offer", attachments: [] })]);
  const label = within(log).getByText("Opening message");
  expect(label.closest(".chat-opening")).not.toBeNull();
  expect(label.closest(".chat-user-body")).toBeNull();
  expect(within(log).queryByText("Brief text")).toBeNull();
  // The skeleton block (96px tall) made the transcript jump when the label arrived.
  expect(log.querySelector('[class*="h-24"]')).toBeNull();
});

const sentWith = (fields: Record<string, unknown>) => {
  const base = item("user").attachments[0]!;
  const attachment = { ...base, quote: "Quoted words", ...fields };
  return mount([item("user", { text: "Ask", origin: null, attachments: [attachment] })]).log;
};

it.each([
  ["an anchor", { anchor: "overview", lines: null }],
  ["a line range", { anchor: null, lines: [3, 5] }],
])("a passage that names %s is quoted above the message", (_name, fields) => {
  expect(sentWith(fields).querySelectorAll(".chat-user-quote")).toHaveLength(1);
});

it("a summary sentence is quoted like a passage", () => {
  const log = sentWith({
    kind: "summary",
    anchor: "s-1234abcd",
    summary_sentence: "s-1234abcd",
    lines: null,
  });
  expect(within(log).getByText("Quoted words").closest(".chat-user-quote")).not.toBeNull();
});

it("a whole document shows its chip alone, with no quote of the file", () => {
  const log = sentWith({ anchor: null, lines: null, stage: null, summary_sentence: null });
  expect(log.querySelector(".chat-user-quote")).toBeNull();
  expect(within(log).queryByText("Quoted words")).toBeNull();
  expect(within(log).getByRole("button", { name: /reference-0.md/ })).toBeTruthy();
});
