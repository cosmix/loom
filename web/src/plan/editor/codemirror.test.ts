import { EditorView } from "@codemirror/view";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import {
  codeLanguages,
  createDiffView,
  createMergeView,
  createSourceEditor,
  type EditorHandle,
} from "./codemirror";

// These tests run the real CodeMirror module; every other editor test mocks it.

beforeAll(() => {
  // jsdom has no text layout, and CodeMirror measures through Range rectangles.
  Range.prototype.getBoundingClientRect = () => new DOMRect(0, 0, 0, 0);
  Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
});

const mounted: { parent: HTMLElement; handle: EditorHandle }[] = [];

function mount<T extends EditorHandle>(create: (parent: HTMLElement) => T) {
  const parent = document.createElement("div");
  document.body.append(parent);
  const handle = create(parent);
  mounted.push({ parent, handle });
  return { parent, handle };
}

afterEach(() => {
  for (const { parent, handle } of mounted.splice(0)) {
    handle.destroy();
    parent.remove();
  }
  document.documentElement.classList.remove("dark");
});

const panes = (parent: HTMLElement) => [...parent.querySelectorAll<HTMLElement>(".cm-editor")];
const viewOf = (pane: HTMLElement) => EditorView.findFromDOM(pane) as EditorView;
const contentOf = (pane: HTMLElement) => pane.querySelector<HTMLElement>(".cm-content")!;
const linesOf = (pane: HTMLElement) => [...pane.querySelectorAll<HTMLElement>(".cm-line")];

function press(target: HTMLElement, init: KeyboardEventInit): KeyboardEvent {
  const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(event);
  return event;
}

const DELETE = { key: "Delete", keyCode: 46 };

function source(doc: string, extra: { readOnly?: boolean } = {}) {
  const onChange = vi.fn();
  const onSave = vi.fn();
  const { parent, handle } = mount((host) =>
    createSourceEditor(host, { doc, onChange, onSave, ...extra }),
  );
  const [pane] = panes(parent);
  return { parent, handle, pane, onChange, onSave };
}

describe("source editor", () => {
  it("saves on Mod-s and stops the browser's save dialog", () => {
    const { pane, onSave } = source("# A\n");
    const event = press(contentOf(pane), { key: "s", ctrlKey: true });
    expect(onSave).toHaveBeenCalledTimes(1);
    expect(event.defaultPrevented).toBe(true);
  });

  it("reports one change per user edit", () => {
    const { pane, onChange } = source("abc");
    press(contentOf(pane), DELETE);
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenLastCalledWith("bc");
    press(contentOf(pane), DELETE);
    expect(onChange).toHaveBeenCalledTimes(2);
    expect(onChange).toHaveBeenLastCalledWith("c");
  });

  it("blocks edits while read-only", () => {
    const { handle, pane, onChange } = source("abc", { readOnly: true });
    press(contentOf(pane), DELETE);
    expect(handle.getText()).toBe("abc");
    expect(onChange).not.toHaveBeenCalled();
    expect(contentOf(pane).getAttribute("contenteditable")).toBe("false");
  });

  it("leaves Tab and Shift-Tab to the browser so focus can leave the editor", () => {
    const { handle, pane, onChange } = source("- item");
    for (const shiftKey of [false, true]) {
      const event = press(contentOf(pane), { key: "Tab", keyCode: 9, shiftKey });
      expect(event.defaultPrevented).toBe(false);
    }
    expect(handle.getText()).toBe("- item");
    expect(onChange).not.toHaveBeenCalled();
  });

  it("names the textbox", () => {
    const { pane } = source("a");
    expect(contentOf(pane).getAttribute("aria-label")).toBe("Document source");
  });

  it("clamps scrollToLine into the document", () => {
    const { handle, pane } = source("one\ntwo\nthree");
    const head = () => viewOf(pane).state.selection.main.head;
    handle.scrollToLine(2);
    expect(head()).toBe(4);
    handle.scrollToLine(99);
    expect(head()).toBe(8);
    handle.scrollToLine(-3);
    expect(head()).toBe(0);
    handle.scrollToLine(3.9);
    expect(head()).toBe(8);
    handle.scrollToLine(Number.NaN);
    expect(head()).toBe(0);
  });

  it("scrolls a diff to its first changed chunk on the after side", () => {
    const before = "one\ntwo\nthree\nfour";
    const after = "one\ntwo\nTHREE\nfour\nfive";
    const head = (pane: HTMLElement) => viewOf(pane).state.selection.main.head;
    const side = mount((host) => createDiffView(host, { before, after, unified: false }));
    side.handle.scrollToFirstChange();
    // Line 3 of the after text starts at offset 8; the before pane keeps its selection.
    expect(panes(side.parent).map(head)).toEqual([0, 8]);
    const unified = mount((host) => createDiffView(host, { before, after, unified: true }));
    unified.handle.scrollToFirstChange();
    expect(head(panes(unified.parent)[0]!)).toBe(8);

    // A chunk that removes the last line starts one past the after text; the line is clamped.
    const cut = mount((host) => createDiffView(host, { before, after: "one\ntwo", unified: true }));
    cut.handle.scrollToFirstChange();
    expect(head(panes(cut.parent)[0]!)).toBe(4);
  });

  it("leaves the selection alone when nothing changed or the editor is no diff", () => {
    const head = (pane: HTMLElement) => viewOf(pane).state.selection.main.head;
    const same = mount((host) =>
      createDiffView(host, { before: "one\ntwo", after: "one\ntwo", unified: true }),
    );
    viewOf(panes(same.parent)[0]!).dispatch({ selection: { anchor: 5 } });
    same.handle.scrollToFirstChange();
    expect(head(panes(same.parent)[0]!)).toBe(5);
    const { handle, pane } = source("one\ntwo");
    viewOf(pane).dispatch({ selection: { anchor: 5 } });
    handle.scrollToFirstChange();
    expect(head(pane)).toBe(5);
  });

  it("maps a selection to document lines and ignores one outside the editor", () => {
    const { handle, pane } = source("one\ntwo\nthree");
    const [first, second, third] = linesOf(pane);
    const inside = document.createRange();
    inside.setStart(first!, 0);
    inside.setEnd(second!, second!.childNodes.length);
    expect(handle.rangeLines(inside)).toEqual({ start: 1, end: 2 });

    // A selection that ends at the start of line 3 does not cover line 3.
    const toLineStart = document.createRange();
    toLineStart.setStart(first!, 0);
    toLineStart.setEnd(third!, 0);
    expect(handle.rangeLines(toLineStart)).toEqual({ start: 1, end: 2 });

    const stray = document.createElement("p");
    stray.textContent = "elsewhere";
    document.body.append(stray);
    const outside = document.createRange();
    outside.selectNodeContents(stray);
    expect(handle.rangeLines(outside)).toBeNull();
    stray.remove();
  });
});

describe("markdown fence languages", () => {
  it("maps yaml and yml fences to the YAML language", () => {
    for (const info of ["yaml", "yml", " YAML ", "Yml\n"]) {
      const description = codeLanguages(info);
      expect(description?.name).toBe("yaml");
      expect(description?.support?.language.name).toBe("yaml");
    }
  });

  it("leaves other fences to the Markdown parser", () => {
    expect(codeLanguages("js")).toBeNull();
    expect(codeLanguages("")).toBeNull();
  });
});

describe("colour scheme", () => {
  const dark = (pane: HTMLElement) => viewOf(pane).state.facet(EditorView.darkTheme);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

  it("starts on the page's scheme and follows the .dark class", async () => {
    document.documentElement.classList.add("dark");
    const { pane } = source("a");
    expect(dark(pane)).toBe(true);
    document.documentElement.classList.remove("dark");
    await vi.waitFor(() => expect(dark(pane)).toBe(false));
    document.documentElement.classList.add("dark");
    await vi.waitFor(() => expect(dark(pane)).toBe(true));
  });

  it("switches both panes of a merge view and of a side-by-side diff", async () => {
    const merge = mount((host) => createMergeView(host, { left: "a", right: "b" }));
    const diff = mount((host) => createDiffView(host, { before: "a", after: "b", unified: false }));
    const all = [...panes(merge.parent), ...panes(diff.parent)];
    expect(all).toHaveLength(4);
    document.documentElement.classList.add("dark");
    await vi.waitFor(() => expect(all.map(dark)).toEqual([true, true, true, true]));
  });

  it("switches a unified diff", async () => {
    const { parent } = mount((host) =>
      createDiffView(host, { before: "a", after: "b", unified: true }),
    );
    document.documentElement.classList.add("dark");
    await vi.waitFor(() => expect(dark(panes(parent)[0]!)).toBe(true));
  });

  it("stops following the page once the editor is destroyed", async () => {
    // Not through mount(): destroying twice (here and in cleanup) is not part of the contract.
    const parent = document.createElement("div");
    document.body.append(parent);
    const handle = createSourceEditor(parent, { doc: "a", onChange: vi.fn(), onSave: vi.fn() });
    const view = viewOf(panes(parent)[0]!);
    handle.destroy();
    parent.remove();
    document.documentElement.classList.add("dark");
    await settle();
    expect(view.state.facet(EditorView.darkTheme)).toBe(false);
  });
});

describe("merge view", () => {
  function merge(readOnly: boolean) {
    const onChange = vi.fn();
    const onSave = vi.fn();
    const made = mount((host) =>
      createMergeView(host, { left: "abc", right: "abc", readOnly, onChange, onSave }),
    );
    const [left, right] = panes(made.parent);
    return { ...made, left: left!, right: right!, onChange, onSave };
  }

  it("names both panes and attributes only the editable side", () => {
    const { handle, left, right } = merge(false);
    expect(contentOf(left).getAttribute("aria-label")).toBe("Version on disk");
    expect(contentOf(right).getAttribute("aria-label")).toBe("Your version");
    const onLeft = document.createRange();
    onLeft.selectNodeContents(linesOf(left)[0]!);
    expect(handle.rangeLines(onLeft)).toBeNull();
    const onRight = document.createRange();
    onRight.selectNodeContents(linesOf(right)[0]!);
    expect(handle.rangeLines(onRight)).toEqual({ start: 1, end: 1 });
  });

  it("saves on Mod-s in the editable side", () => {
    const { right, onSave } = merge(false);
    const event = press(contentOf(right), { key: "s", ctrlKey: true });
    expect(onSave).toHaveBeenCalledTimes(1);
    expect(event.defaultPrevented).toBe(true);
  });

  it("makes the editable side read-only and editable again without a rebuild", () => {
    const { handle, parent, right, onChange } = merge(true);
    expect(parent.querySelector(".cm-merge-revert")).toBeNull();
    press(contentOf(right), DELETE);
    expect(handle.getText()).toBe("abc");

    handle.setReadOnly(false);
    expect(parent.querySelector(".cm-merge-revert")).not.toBeNull();
    press(contentOf(right), DELETE);
    expect(handle.getText()).toBe("bc");
    expect(onChange).toHaveBeenCalledTimes(1);

    handle.setReadOnly(true);
    expect(parent.querySelector(".cm-merge-revert")).toBeNull();
    press(contentOf(right), DELETE);
    expect(handle.getText()).toBe("bc");
    expect(panes(parent)).toHaveLength(2);
  });
});

describe("diff view", () => {
  it("names the two panes and tells the before side from the after side", () => {
    const { handle, parent } = mount((host) =>
      createDiffView(host, { before: "a\nb", after: "a\nc", unified: false }),
    );
    const [before, after] = panes(parent);
    expect(contentOf(before!).getAttribute("aria-label")).toBe("Before");
    expect(contentOf(after!).getAttribute("aria-label")).toBe("After");
    const range = (pane: HTMLElement) => {
      const found = document.createRange();
      found.selectNodeContents(linesOf(pane)[1]!);
      return found;
    };
    expect(handle.rangeLines(range(before!))).toEqual({ start: 2, end: 2, side: "before" });
    expect(handle.rangeLines(range(after!))).toEqual({ start: 2, end: 2, side: "after" });
  });

  it("names a unified diff and is read-only", () => {
    const { handle, parent } = mount((host) =>
      createDiffView(host, { before: "a", after: "b", unified: true }),
    );
    const [pane] = panes(parent);
    expect(contentOf(pane!).getAttribute("aria-label")).toBe("Changes");
    press(contentOf(pane!), DELETE);
    expect(handle.getText()).toBe("b");
  });
});

describe("unified diff selections", () => {
  function unifiedDiff() {
    const { handle, parent } = mount((host) =>
      createDiffView(host, { before: "a\nb\nc", after: "a\nc", unified: true }),
    );
    return { handle, pane: panes(parent)[0]! };
  }

  it("attributes no line to a selection inside a removed chunk", () => {
    const { handle, pane } = unifiedDiff();
    const removed = pane.querySelector<HTMLElement>(".cm-deletedChunk del");
    expect(removed?.textContent).toBe("b");
    const inside = document.createRange();
    inside.setStart(removed!.firstChild!, 0);
    inside.setEnd(removed!.firstChild!, 1);
    expect(handle.rangeLines(inside)).toBeNull();

    // A selection that starts on kept text and ends in the chunk is not attributed either.
    const across = document.createRange();
    across.setStart(linesOf(pane)[0]!, 0);
    across.setEnd(removed!.firstChild!, 1);
    expect(handle.rangeLines(across)).toBeNull();
  });

  it("still maps a selection on kept text", () => {
    const { handle, pane } = unifiedDiff();
    const kept = linesOf(pane);
    expect(kept.map((line) => line.textContent)).toEqual(["a", "c"]);
    const onLast = document.createRange();
    onLast.selectNodeContents(kept[1]!);
    expect(handle.rangeLines(onLast)).toEqual({ start: 2, end: 2 });
  });
});

describe("line numbers", () => {
  it("show in the merge view and in both diff layouts", () => {
    const merge = mount((host) => createMergeView(host, { left: "a", right: "b" }));
    const side = mount((host) => createDiffView(host, { before: "a", after: "b", unified: false }));
    const unified = mount((host) =>
      createDiffView(host, { before: "a", after: "b", unified: true }),
    );
    expect(merge.parent.querySelectorAll(".cm-lineNumbers")).toHaveLength(2);
    expect(side.parent.querySelectorAll(".cm-lineNumbers")).toHaveLength(2);
    expect(unified.parent.querySelectorAll(".cm-lineNumbers")).toHaveLength(1);
  });
});

describe("range text", () => {
  it("returns the document slice of a range, and null outside the editor", () => {
    const { handle, pane } = source("one\ntwo\nthree");
    const [first, second] = linesOf(pane);
    const range = document.createRange();
    range.setStart(first!, 0);
    range.setEnd(second!, second!.childNodes.length);
    expect(handle.rangeText(range)).toBe("one\ntwo");
    const stray = document.createElement("p");
    stray.textContent = "elsewhere";
    document.body.append(stray);
    range.selectNodeContents(stray);
    expect(handle.rangeText(range)).toBeNull();
    stray.remove();
  });

  it("reads the editable side of a merge view and both sides of a diff", () => {
    const merge = mount((host) => createMergeView(host, { left: "old", right: "new" }));
    const [left, right] = panes(merge.parent);
    const range = (pane: HTMLElement) => {
      const found = document.createRange();
      found.selectNodeContents(linesOf(pane)[0]!);
      return found;
    };
    expect(merge.handle.rangeText(range(left!))).toBeNull();
    expect(merge.handle.rangeText(range(right!))).toBe("new");
    const diff = mount((host) => createDiffView(host, { before: "a", after: "b", unified: false }));
    const [before, after] = panes(diff.parent);
    expect(diff.handle.rangeText(range(before!))).toBe("a");
    expect(diff.handle.rangeText(range(after!))).toBe("b");
  });
});

describe("line endings", () => {
  it("keeps CRLF through reads, Enter and setText", () => {
    const { handle, pane, onChange } = source("a\r\nb");
    expect(handle.getText()).toBe("a\r\nb");
    const view = viewOf(pane);
    view.dispatch({ selection: { anchor: view.state.doc.length } });
    press(contentOf(pane), { key: "Enter", keyCode: 13 });
    expect(handle.getText()).toBe("a\r\nb\r\n");
    expect(onChange).toHaveBeenLastCalledWith("a\r\nb\r\n");
    handle.setText("x\r\ny");
    expect(handle.getText()).toBe("x\r\ny");
    expect(view.state.doc.lines).toBe(2);
  });

  it("rewrites pasted LF text to the document's break", () => {
    const { handle, pane } = source("a\r\nb");
    const clipboardData = { getData: () => "x\ny\r\nz\rw" };
    contentOf(pane).dispatchEvent(Object.assign(new Event("paste"), { clipboardData }));
    expect(handle.getText()).toBe("x\r\ny\r\nz\r\nwa\r\nb");
  });

  it("leaves a plain LF document with the default splitting", () => {
    const { pane } = source("a\nb\n");
    expect(viewOf(pane).state.lineBreak).toBe("\n");
    expect(viewOf(pane).state.doc.lines).toBe(3);
  });
});

describe("format shortcuts", () => {
  it("are installed in the source editor and not in the merge view", () => {
    const { handle, pane } = source("word");
    viewOf(pane).dispatch({ selection: { anchor: 0, head: 4 } });
    press(contentOf(pane), { key: "b", ctrlKey: true });
    expect(handle.getText()).toBe("**word**");

    const merge = mount((host) => createMergeView(host, { left: "word", right: "word" }));
    const right = panes(merge.parent)[1]!;
    viewOf(right).dispatch({ selection: { anchor: 0, head: 4 } });
    press(contentOf(right), { key: "b", ctrlKey: true });
    expect(merge.handle.getText()).toBe("word");
  });
});
