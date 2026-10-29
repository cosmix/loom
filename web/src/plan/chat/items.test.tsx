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

it("a user message sizes to its text and sits at the right edge", () => {
  const { log } = mount([item("user", { text: "ok", origin: null, attachments: [] })]);
  const block = within(log).getByText("ok").closest('[class*="max-w-"]')!;
  expect(block.className).toContain("w-fit");
  expect(block.className).toContain("ml-auto");
});

it("the opening message header is inline text while its label loads", () => {
  const { log } = mount([item("user", { text: "Brief text", origin: "start", attachments: [] })]);
  expect(within(log).getByText("Opening message")).toBeTruthy();
  // The skeleton block (96px tall) made the transcript jump when the label arrived.
  expect(log.querySelector('[class*="h-24"]')).toBeNull();
});
