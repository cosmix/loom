import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { afterEach, expect, it } from "vitest";

import { RunOutput } from "./run-output";
import { runOutputAtom } from "./use-run";

afterEach(cleanup);

it("renders untrusted output lines as monospace text", () => {
  const store = createStore();
  store.set(runOutputAtom, {
    ...store.get(runOutputAtom),
    start: ["<img src=x onerror=alert(1)>", "", "<script>alert(2)</script>"],
    commit: ["different job"],
  });
  const { container } = render(
    <Provider store={store}>
      <RunOutput job="start" />
    </Provider>,
  );
  const output = screen.getByLabelText("start output");
  expect(output.tagName).toBe("PRE");
  expect(output.classList.contains("font-mono")).toBe(true);
  expect(output.textContent).toBe("<img src=x onerror=alert(1)>\n\n<script>alert(2)</script>");
  expect(container.querySelector("img, script")).toBeNull();
});

it("follows output until the user scrolls up and resumes at the bottom", () => {
  const store = createStore();
  render(
    <Provider store={store}>
      <RunOutput job="start" />
    </Provider>,
  );
  const output = screen.getByLabelText("start output");
  Object.defineProperties(output, {
    scrollHeight: { configurable: true, value: 600 },
    clientHeight: { configurable: true, value: 200 },
  });
  act(() => store.set(runOutputAtom, { ...store.get(runOutputAtom), start: ["first"] }));
  expect(output.scrollTop).toBe(600);

  output.scrollTop = 50;
  fireEvent.scroll(output);
  act(() => store.set(runOutputAtom, { ...store.get(runOutputAtom), start: ["first", "second"] }));
  expect(output.scrollTop).toBe(50);

  output.scrollTop = 400;
  fireEvent.scroll(output);
  Object.defineProperty(output, "scrollHeight", { configurable: true, value: 800 });
  act(() =>
    store.set(runOutputAtom, { ...store.get(runOutputAtom), start: ["first", "second", "third"] }),
  );
  expect(output.scrollTop).toBe(800);
});
