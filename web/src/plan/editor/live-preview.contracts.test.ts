import { undo } from "@codemirror/commands";
import { forceParsing } from "@codemirror/language";
import { StateEffect } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { afterEach, beforeAll, expect, it, vi } from "vitest";

import {
  createSourceEditor,
  type SourceEditorHandle,
  type SourceEditorOptions,
} from "./codemirror";

// Contracts for the live preview: the real CodeMirror chunk, read through the DOM and the view.

beforeAll(() => {
  // jsdom has no text layout, and CodeMirror measures through Range rectangles.
  Range.prototype.getBoundingClientRect = () => new DOMRect(0, 0, 0, 0);
  Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
});

const mounted: { parent: HTMLElement; handle: SourceEditorHandle }[] = [];

afterEach(() => {
  for (const { parent, handle } of mounted.splice(0)) {
    handle.destroy();
    parent.remove();
  }
});

type Extra = Partial<Pick<SourceEditorOptions, "readOnly" | "onFollowLink" | "mode">>;

/** A source editor on `doc` with the cursor at `cursor`, opened in its default mode. */
function edit(doc: string, cursor = 0, extra: Extra = {}) {
  const parent = document.createElement("div");
  document.body.append(parent);
  const onChange = vi.fn();
  const handle = createSourceEditor(parent, { doc, onChange, onSave: vi.fn(), ...extra });
  mounted.push({ parent, handle });
  const editor = parent.querySelector<HTMLElement>(".cm-editor")!;
  const view = EditorView.findFromDOM(editor) as EditorView;
  expect(handle.mode()).toBe(extra.mode ?? "preview");
  view.dispatch({ selection: { anchor: cursor } });
  const lines = () => [...view.contentDOM.querySelectorAll<HTMLElement>(".cm-line")];
  const line = (n: number) => lines()[n - 1]!.textContent;
  const text = () => view.contentDOM.textContent ?? "";
  const moveTo = (n: number) =>
    view.dispatch({ selection: { anchor: view.state.doc.line(n).from } });
  return { parent, handle, view, onChange, lines, line, text, moveTo };
}

/** Puts the cursor on every line in turn, as a reader moving through the document would. */
function walk(view: EditorView): void {
  for (let n = 1; n <= view.state.doc.lines; n += 1) {
    view.dispatch({ selection: { anchor: view.state.doc.line(n).from } });
  }
}

function press(view: EditorView, key: string): KeyboardEvent {
  const event = new KeyboardEvent("keydown", {
    key,
    ctrlKey: true,
    bubbles: true,
    cancelable: true,
  });
  view.contentDOM.dispatchEvent(event);
  return event;
}

const box = (view: EditorView) =>
  view.contentDOM.querySelector<HTMLInputElement>("input[type=checkbox]");

/** Records every later document change as [fromA, toA, inserted]. */
function recordChanges(view: EditorView): [number, number, string][] {
  const seen: [number, number, string][] = [];
  const listener = EditorView.updateListener.of((update) =>
    update.changes.iterChanges((fromA, toA, _fromB, _toB, inserted) =>
      seen.push([fromA, toA, inserted.toString()]),
    ),
  );
  view.dispatch({ effects: StateEffect.appendConfig.of(listener) });
  return seen;
}

const SAMPLE = [
  "<!-- loom METADATA -->",
  "# Heading  ",
  "",
  "Some **bold** and *soft* text with trailing spaces   ",
  "",
  "- [ ] open task",
  "- [x] done task",
  "",
  "| a | b |",
  "| - | - |",
  "| 1 | 2 |",
  "",
  "~~~yaml",
  "loom:",
  "  version: 1",
  "~~~",
  "",
].join("\n");

it("toggling modes and saving unedited returns identical bytes", () => {
  const { handle, view, onChange } = edit(SAMPLE);
  walk(view);
  handle.setMode("source");
  expect(handle.mode()).toBe("source");
  walk(view);
  handle.setMode("preview");
  expect(handle.mode()).toBe("preview");
  walk(view);
  expect(handle.getText()).toBe(SAMPLE);
  expect(onChange).not.toHaveBeenCalled();
});

it("markers hide off the cursor line and show on it", () => {
  const { line, moveTo, handle } = edit("# Title\n\nsome **bold** text", 0);
  expect(line(3)).toBe("some bold text");
  moveTo(3);
  expect(line(3)).toBe("some **bold** text");
  expect(handle.getText()).toBe("# Title\n\nsome **bold** text");
});

it("html comments stay visible in live preview", () => {
  const doc = "intro\n\n<!-- loom METADATA -->\n\nafter";
  const { line, handle } = edit(doc, 0);
  expect(handle.mode()).toBe("preview");
  expect(line(3)).toBe("<!-- loom METADATA -->");
  expect(handle.getText()).toBe(doc);
});

it("clicking a task box toggles it and undo restores it", () => {
  const doc = "- [ ] item\n\nother";
  const { handle, view, line } = edit(doc, doc.length);
  expect(line(1)).toContain("item");
  expect(box(view)).not.toBeNull();
  const changes = recordChanges(view);
  box(view)!.click();
  expect(handle.getText()).toBe("- [x] item\n\nother");
  expect(changes).toEqual([[3, 4, "x"]]);
  expect(line(1)).toContain("item");
  undo(view);
  expect(handle.getText()).toBe(doc);
});

it("bold toggles around the selection and undoes in one step", () => {
  const { handle, view } = edit("a word here");
  view.dispatch({ selection: { anchor: 2, head: 6 } });
  handle.format("bold");
  expect(handle.getText()).toBe("a **word** here");
  undo(view);
  expect(handle.getText()).toBe("a word here");
  view.dispatch({ selection: { anchor: 2, head: 6 } });
  handle.format("bold");
  expect(handle.getText()).toBe("a **word** here");
  handle.format("bold");
  expect(handle.getText()).toBe("a word here");
  undo(view);
  expect(handle.getText()).toBe("a **word** here");
});

it("switching mode keeps the cursor and undo history", () => {
  const { handle, view } = edit("one two", 3);
  view.dispatch({
    changes: { from: 3, insert: " word" },
    selection: { anchor: 8 },
    userEvent: "input.type",
  });
  expect(handle.getText()).toBe("one word two");
  const head = view.state.selection.main.head;
  handle.setMode("source");
  expect(handle.mode()).toBe("source");
  expect(view.state.selection.main.head).toBe(head);
  undo(view);
  expect(handle.getText()).toBe("one two");
});

it("range text includes hidden markers", () => {
  const { handle, lines, line } = edit("x\n\nsome **bold** text", 0);
  expect(line(3)).toBe("some bold text");
  const range = document.createRange();
  range.selectNodeContents(lines()[2]!);
  expect(handle.rangeText(range)).toBe("some **bold** text");
});

const ENDINGS: [string, string][] = [
  ["# title\r\n\r\nbody  \r\n", "body"],
  ["a\rb\r", "b"],
  ["a\r\nb\nc\r", "b"],
];

it("line endings survive untouched and a one word edit changes only that word", () => {
  for (const [doc, word] of ENDINGS) {
    const { handle, view, onChange } = edit(doc);
    handle.setMode("source");
    handle.setMode("preview");
    walk(view);
    expect(handle.getText()).toBe(doc);
    expect(onChange).not.toHaveBeenCalled();

    const { state } = view;
    const lineNo = [...Array(state.doc.lines).keys()]
      .map((i) => i + 1)
      .find((n) => state.doc.line(n).text.includes(word))!;
    const from = state.doc.line(lineNo).from + state.doc.line(lineNo).text.indexOf(word);
    view.dispatch({ changes: { from, to: from + word.length, insert: "zz" } });
    expect(handle.getText()).toBe(doc.replace(word, "zz"));
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenLastCalledWith(handle.getText());
  }
});

it("Mod-b, Mod-i and Mod-k run in both modes", () => {
  const { handle, view } = edit("a word here");
  const cases: [string, RegExp][] = [
    ["b", /^a \*\*word\*\* here$/],
    ["i", /^a \*word\* here$/],
    ["k", /^a \[word\]\([^)]+\) here$/],
  ];
  for (const mode of ["preview", "source"] as const) {
    handle.setMode(mode);
    for (const [key, expected] of cases) {
      view.dispatch({ selection: { anchor: 2, head: 6 } });
      const event = press(view, key);
      expect(event.defaultPrevented).toBe(true);
      expect(handle.getText()).toMatch(expected);
      undo(view);
      expect(handle.getText()).toBe("a word here");
    }
  }
});

it("fences hide off the block, show inside it and label the language", () => {
  const doc = "intro\n\n~~~yaml\na: 1\n~~~\n\nafter";
  const { handle, view, text, moveTo } = edit(doc, 0);
  expect(text()).not.toContain("~~~");
  expect(text()).toContain("a: 1");
  const label = view.contentDOM.querySelector(".cm-lp-fence-label");
  expect(label?.textContent).toBe("yaml");
  expect(handle.getText()).toBe(doc);
  moveTo(4);
  expect(text()).toContain("~~~");
  expect(handle.getText()).toBe(doc);
});

it("table pipes are dimmed and stay in the text", () => {
  const doc = "intro\n\n| a | b |\n| - | - |\n| 1 | 2 |";
  const { handle, view, text } = edit(doc, 0);
  expect(view.contentDOM.querySelectorAll(".cm-lp-dim").length).toBeGreaterThan(0);
  const pipes = (value: string) => value.split("|").length - 1;
  expect(pipes(text())).toBe(pipes(doc));
  expect(view.contentDOM.querySelector("table")).toBeNull();
  expect(handle.getText()).toBe(doc);
});

it("a titled link reads as its label off the cursor line", () => {
  const source = 'see [label](https://example.com/x "title") now';
  const { line, moveTo, handle } = edit(`intro\n\n${source}`, 0);
  expect(line(3)).toBe("see label now");
  moveTo(3);
  expect(line(3)).toBe(source);
  expect(handle.getText()).toBe(`intro\n\n${source}`);
});

it("decorations follow a finished background parse", () => {
  const filler = Array.from({ length: 400 }, (_, i) => `plain line ${i + 1}`).join("\n");
  // The blank line ends the filler paragraph, so the first parse stops before the bold line.
  const doc = `${filler}\n\nsome **bold** text\n\n~~~yaml\na: 1\n~~~\nend`;
  const { handle, view, lines, text } = edit(doc, 0);
  // jsdom draws only the lines near the scroll target; scrolling moves neither text nor cursor.
  view.dispatch({ effects: EditorView.scrollIntoView(doc.indexOf("some **"), { y: "start" }) });
  const selection = view.state.selection;
  forceParsing(view, view.state.doc.length, 10_000);
  expect(view.state.selection.eq(selection)).toBe(true);
  const bold = lines().find((element) => element.textContent?.includes("bold"));
  expect(bold?.textContent).toBe("some bold text");
  expect(text()).toContain("a: 1");
  expect(text()).not.toContain("~~~");
  expect(handle.getText()).toBe(doc);
});

it("line numbers show in source only and validation marks in both modes", () => {
  const { handle, parent } = edit("a\nb\nc");
  handle.setGutterMarks([2]);
  const has = (selector: string) => parent.querySelector(selector) !== null;
  expect(has(".cm-line-mark")).toBe(true);
  expect(has(".cm-lineNumbers")).toBe(false);
  handle.setMode("source");
  expect(has(".cm-line-mark")).toBe(true);
  expect(has(".cm-lineNumbers")).toBe(true);
  handle.setMode("preview");
  expect(has(".cm-line-mark")).toBe(true);
  expect(has(".cm-lineNumbers")).toBe(false);
});

it("read-only editors refuse formatting and task toggles", () => {
  const doc = "- [ ] item\n\nother";
  const { handle, view } = edit(doc, doc.length, { readOnly: true });
  view.dispatch({ selection: { anchor: doc.length - 5, head: doc.length } });
  handle.format("bold");
  expect(handle.getText()).toBe(doc);
  const event = press(view, "b");
  expect(event.defaultPrevented).toBe(true);
  expect(handle.getText()).toBe(doc);
  expect(box(view)).not.toBeNull();
  box(view)!.click();
  expect(handle.getText()).toBe(doc);
  handle.setMode("source");
  expect(handle.mode()).toBe("source");
  expect(handle.getText()).toBe(doc);
});

it("mod-click follows a link and a plain click does not", () => {
  const doc = "intro\n\nsee [label](notes.md) now";
  const onFollowLink = vi.fn();
  const { handle, view } = edit(doc, 0, { onFollowLink });
  // The label's own element when CodeMirror wraps it, else its text node.
  const label = (): Node => {
    const { node } = view.domAtPos(doc.indexOf("label") + 2);
    const element = node.parentElement!;
    return element.classList.contains("cm-line") ? node : element;
  };
  const mousedown = (ctrlKey: boolean) =>
    label().dispatchEvent(
      new MouseEvent("mousedown", { bubbles: true, cancelable: true, ctrlKey, button: 0 }),
    );
  expect(label().textContent).toBe("label");
  mousedown(false);
  expect(onFollowLink).not.toHaveBeenCalled();
  mousedown(true);
  expect(onFollowLink).toHaveBeenCalledTimes(1);
  expect(onFollowLink).toHaveBeenCalledWith("notes.md");
  expect(handle.getText()).toBe(doc);
});
