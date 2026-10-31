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

it("a thinking marker without a duration says Thought, not a measured 0s", () => {
  const { log } = mount([item("thinking", { id: "unknown", duration_ms: null })]);
  expect(within(log).getByText("Thought")).toBeTruthy();
  expect(within(log).queryByText(/for 0s/)).toBeNull();
});

it.each([
  [1500, "Thought for 1.5s"],
  [0, "Thought for 0s"],
])("a thinking marker measured at %i ms still reports it", (duration_ms, words) => {
  const { log } = mount([item("thinking", { id: "known", duration_ms })]);
  expect(within(log).getByText(words)).toBeTruthy();
});

it("a user message is set apart by its rule and face, not shrink-wrapped into a bubble", () => {
  const { log } = mount([item("user", { text: "ok", origin: null, attachments: [] })]);
  const block = within(log).getByText("ok").closest(".chat-user")!;
  expect(block.className).not.toContain("w-fit");
  expect(block.className).not.toContain("ml-auto");
});

it("the opening message header is inline text while its label loads", () => {
  const { log } = mount([item("user", { text: "Brief text", origin: "start", attachments: [] })]);
  expect(within(log).getByText("Opening message")).toBeTruthy();
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
