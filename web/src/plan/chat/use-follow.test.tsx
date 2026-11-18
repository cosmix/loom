import { cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";

import { item, mount, turn } from "./transcript-test-kit";

afterEach(cleanup);

it("pill when scrolled up, follow at bottom", () => {
  const first = turn([item("text", { id: "first", markdown: "First reply" })]);
  const h = mount([], { turns: [first] });
  Object.defineProperties(h.log, {
    scrollHeight: { configurable: true, value: 1000 },
    clientHeight: { configurable: true, value: 200 },
  });
  // The reader was at the end, then wheeled towards the start: only that leaves the end.
  h.log.scrollTop = 800;
  fireEvent.scroll(h.log);
  fireEvent.wheel(h.log);
  h.log.scrollTop = 0;
  fireEvent.scroll(h.log);
  const second = {
    ...first,
    items: [...first.items, item("text", { id: "second", markdown: "Second reply" })],
  };
  h.update([second]);
  expect(h.log.scrollTop).toBe(0);
  fireEvent.click(screen.getByRole("button", { name: "New activity" }));
  expect(h.log.scrollTop).toBe(1000);
  expect(screen.queryByRole("button", { name: "New activity" })).toBeNull();
  h.log.scrollTop = 800;
  fireEvent.scroll(h.log);
  Object.defineProperty(h.log, "scrollHeight", { configurable: true, value: 1200 });
  h.update([
    { ...second, items: [...second.items, item("text", { id: "third", markdown: "Third reply" })] },
  ]);
  expect(h.log.scrollTop).toBe(1200);
  expect(screen.queryByRole("button", { name: "New activity" })).toBeNull();
});

it("the end moving away with no input from the reader is not the reader leaving", () => {
  const first = turn([item("text", { id: "first", markdown: "First reply" })]);
  const h = mount([], { turns: [first] });
  Object.defineProperties(h.log, {
    scrollHeight: { configurable: true, value: 1000 },
    clientHeight: { configurable: true, value: 200 },
  });
  h.log.scrollTop = 800;
  fireEvent.scroll(h.log);
  // The composer grew: the scroller is shorter, and a late scroll event finds the end 120px off.
  Object.defineProperty(h.log, "clientHeight", { configurable: true, value: 80 });
  fireEvent.scroll(h.log);
  h.update([
    { ...first, items: [...first.items, item("text", { id: "second", markdown: "Second reply" })] },
  ]);
  expect(h.log.scrollTop).toBe(1000);
  expect(screen.queryByRole("button", { name: "New activity" })).toBeNull();
});

it("a scroll towards the start with no input and no layout change is the reader's", () => {
  const first = turn([item("text", { id: "first", markdown: "First reply" })]);
  const h = mount([], { turns: [first] });
  Object.defineProperties(h.log, {
    scrollHeight: { configurable: true, value: 1000 },
    clientHeight: { configurable: true, value: 200 },
  });
  h.log.scrollTop = 800;
  fireEvent.scroll(h.log);
  // Find in page or a screen reader moved the transcript: no wheel, key or pointer event fired.
  h.log.scrollTop = 300;
  fireEvent.scroll(h.log);
  h.update([
    { ...first, items: [...first.items, item("text", { id: "second", markdown: "Second reply" })] },
  ]);
  expect(h.log.scrollTop).toBe(300);
  expect(screen.getByRole("button", { name: "New activity" })).toBeTruthy();
});
