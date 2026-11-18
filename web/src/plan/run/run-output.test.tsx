import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { afterEach, expect, it } from "vitest";

import { RunOutput } from "./run-output";
import { appendRunOutput, runOutputAtom } from "./use-run";

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
  expect(output.textContent).toBe("<img src=x onerror=alert(1)>\n\n<script>alert(2)</script>\n");
  expect(container.querySelector("img, script")).toBeNull();
});

it("is a named log region that wraps a long unbroken token", () => {
  const store = createStore();
  render(
    <Provider store={store}>
      <RunOutput job="commit" />
    </Provider>,
  );
  const output = screen.getByRole("log", { name: "commit output" });
  expect(output.classList.contains("[overflow-wrap:anywhere]")).toBe(true);
});

it("shows format characters in command output as visible markers", () => {
  const store = createStore();
  store.set(runOutputAtom, {
    ...store.get(runOutputAtom),
    abandon: ["git branch -D loom/\u202Efdp.sh"],
  });
  render(
    <Provider store={store}>
      <RunOutput job="abandon" />
    </Provider>,
  );
  const output = screen.getByRole("log", { name: "abandon output" });
  expect(output.textContent).toBe("git branch -D loom/⟨U+202E⟩fdp.sh\n");
});

const frame = (line: string) => ({
  job: "start" as const,
  step: "loom init",
  stream: "stdout" as const,
  line,
  exit_code: null,
  done: false,
});

function mountStart(lines: string[]) {
  const store = createStore();
  store.set(runOutputAtom, { ...store.get(runOutputAtom), start: lines });
  render(
    <Provider store={store}>
      <RunOutput job="start" />
    </Provider>,
  );
  return { store, output: screen.getByRole("log", { name: "start output" }) };
}

it("renders one element per line, keeping an empty line", () => {
  const { output } = mountStart(["first", "", "third"]);
  expect(Array.from(output.children, (child) => child.textContent)).toEqual([
    "first\n",
    "\n",
    "third\n",
  ]);
});

it("leaves the elements of earlier lines untouched when a line is appended", () => {
  const { store, output } = mountStart(["first", "second"]);
  const before = Array.from(output.children);

  act(() => appendRunOutput(store, frame("third")));

  expect(output.children).toHaveLength(3);
  expect(Array.from(output.children).slice(0, 2)).toEqual(before);
  expect(before.map((child) => child.textContent)).toEqual(["first\n", "second\n"]);
});

it("keeps the surviving lines' elements when the 5,000-line cap drops the oldest", () => {
  const lines = Array.from({ length: 5000 }, (_, index) => `line ${index}`);
  const { store, output } = mountStart(lines);
  const [oldest, second, ...rest] = Array.from(output.children);
  const newest = rest.at(-1);

  act(() => appendRunOutput(store, frame("line 5000")));

  expect(output.children).toHaveLength(5000);
  expect(oldest!.isConnected).toBe(false);
  expect(output.children[0]).toBe(second);
  expect(output.children[4998]).toBe(newest);
  expect(output.children[4999]!.textContent).toBe("line 5000\n");
});

it("does not reuse a key for unrelated lines when the list is replaced", () => {
  const { store, output } = mountStart(["old one", "old two"]);
  const before = Array.from(output.children);

  act(() => store.set(runOutputAtom, { ...store.get(runOutputAtom), start: ["new one"] }));

  expect(output.children).toHaveLength(1);
  expect(before.every((child) => !child.isConnected)).toBe(true);
  expect(output.children[0]!.textContent).toBe("new one\n");
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
