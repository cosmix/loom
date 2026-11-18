import { cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";

import transcriptCss from "./chat-transcript.css?raw";
import { item, mount, turn } from "./transcript-test-kit";

afterEach(cleanup);

it("the New activity pill is a raised floating surface, set off from the card it sits on", () => {
  const rule = /(?:^|\n)\.chat-follow \{([^}]*)\}/.exec(transcriptCss)?.[1] ?? "";
  expect(rule).toContain("background: var(--popover);");
  expect(rule).toContain("border: 1px solid var(--popover-border);");
  expect(rule).toContain("box-shadow: var(--plan-shadow), var(--popover-lift);");
});

it("a message the reader sends brings the transcript to its end from above the fold", () => {
  const first = turn([item("text", { id: "first", markdown: "First reply" })]);
  const h = mount([], { turns: [first] });
  Object.defineProperties(h.log, {
    scrollHeight: { configurable: true, value: 1000 },
    clientHeight: { configurable: true, value: 200 },
  });
  // The reader scrolled up, away from the end.
  h.log.scrollTop = 800;
  fireEvent.scroll(h.log);
  fireEvent.wheel(h.log);
  h.log.scrollTop = 0;
  fireEvent.scroll(h.log);
  // A reply below the fold does not move the reader; it raises the pill.
  h.update([
    { ...first, items: [...first.items, item("text", { id: "reply", markdown: "Reply" })] },
  ]);
  expect(h.log.scrollTop).toBe(0);
  expect(screen.getByRole("button", { name: "New activity" })).toBeTruthy();
  // The reader's own message is drawn: the transcript goes to the end and the pill goes.
  h.update([
    {
      ...first,
      items: [
        ...first.items,
        item("text", { id: "reply", markdown: "Reply" }),
        item("user", { id: "sent", text: "My question", origin: null }),
      ],
    },
  ]);
  expect(h.log.scrollTop).toBe(1000);
  expect(screen.queryByRole("button", { name: "New activity" })).toBeNull();
});

it("a closing write's user item is not a message the reader sent", () => {
  const first = turn([item("text", { id: "first", markdown: "First reply" })]);
  const h = mount([], { turns: [first] });
  Object.defineProperties(h.log, {
    scrollHeight: { configurable: true, value: 1000 },
    clientHeight: { configurable: true, value: 200 },
  });
  h.log.scrollTop = 800;
  fireEvent.scroll(h.log);
  fireEvent.wheel(h.log);
  h.log.scrollTop = 0;
  fireEvent.scroll(h.log);
  h.update([
    {
      ...first,
      items: [...first.items, item("user", { id: "closing", text: "Wrap up", origin: "closing" })],
    },
  ]);
  expect(h.log.scrollTop).toBe(0);
  expect(screen.getByRole("button", { name: "New activity" })).toBeTruthy();
});
