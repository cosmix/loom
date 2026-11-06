import { undo } from "@codemirror/commands";
import { forceParsing } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import {
  createSourceEditor,
  type SourceEditorHandle,
  type SourceEditorOptions,
} from "./codemirror";

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

/** A line's text without the fence's language label widget. */
function plain(line: HTMLElement): string {
  const copy = line.cloneNode(true) as HTMLElement;
  for (const label of copy.querySelectorAll(".cm-lp-fence-label")) label.remove();
  return copy.textContent ?? "";
}

/** A source editor on `doc` with the cursor at `cursor` (the end of the document when null). */
function edit(doc: string, cursor: number | null = 0, extra: Extra = {}) {
  const parent = document.createElement("div");
  document.body.append(parent);
  const handle = createSourceEditor(parent, { doc, onChange: vi.fn(), onSave: vi.fn(), ...extra });
  mounted.push({ parent, handle });
  const view = EditorView.findFromDOM(parent.querySelector<HTMLElement>(".cm-editor")!)!;
  view.dispatch({ selection: { anchor: cursor ?? doc.length } });
  const lines = () => [...view.contentDOM.querySelectorAll<HTMLElement>(".cm-line")];
  const texts = () => lines().map(plain);
  const moveTo = (pos: number) => view.dispatch({ selection: { anchor: pos } });
  const q = (selector: string) => view.contentDOM.querySelectorAll(selector);
  return { parent, handle, view, lines, texts, moveTo, q };
}

const at = (doc: string, needle: string): number => doc.indexOf(needle);

describe("marker hiding", () => {
  it("hides heading marks and the space after them off the cursor line", () => {
    const doc = "plain\n\n# Title\n\n## Closed ##";
    const { texts, moveTo } = edit(doc, 0);
    expect(texts()).toEqual(["plain", "", "Title", "", "Closed"]);
    moveTo(at(doc, "# Title"));
    expect(texts()[2]).toBe("# Title");
    expect(texts()[4]).toBe("Closed");
    moveTo(at(doc, "## Closed"));
    expect(texts()[4]).toBe("## Closed ##");
  });

  it("hides emphasis, strikethrough and inline code marks", () => {
    const doc = "x\n\na *i* **b** ~~s~~ `c` end";
    const { texts, moveTo } = edit(doc, 0);
    expect(texts()[2]).toBe("a i b s c end");
    moveTo(doc.length);
    expect(texts()[2]).toBe("a *i* **b** ~~s~~ `c` end");
  });

  it("shows markers on every line a selection range touches, ends included", () => {
    const doc = "**a**\n\n**b**\n\n**c**";
    const { texts, view } = edit(doc, 0);
    view.dispatch({ selection: { anchor: at(doc, "**a**") + 5, head: at(doc, "**b**") } });
    expect(texts()).toEqual(["**a**", "", "**b**", "", "c"]);
  });

  it("draws a link as its label and keeps the label's own marks hidden", () => {
    const doc = "x\n\nsee [**bold** label](https://a.b/c) now";
    const { texts, moveTo } = edit(doc, 0);
    expect(texts()[2]).toBe("see bold label now");
    moveTo(doc.length);
    expect(texts()[2]).toBe("see [**bold** label](https://a.b/c) now");
  });

  it("hides nothing of a reference link, an image or an autolink", () => {
    const doc = "x\n\n[ref] ![alt](img.png) <https://a.b>";
    const { texts } = edit(doc, 0);
    expect(texts()[2]).toBe("[ref] ![alt](img.png) <https://a.b>");
  });

  it("hides nothing inside code, tables, html blocks and comments", () => {
    const doc = [
      "x",
      "",
      "```",
      "**code** [a](b)",
      "```",
      "",
      "| **t** | `u` |",
      "| --- | --- |",
      "",
      "<div>**h**</div>",
      "",
      "<!-- **c** -->",
    ].join("\n");
    const { texts } = edit(doc, 0);
    for (const kept of [
      "**code** [a](b)",
      "| **t** | `u` |",
      "<div>**h**</div>",
      "<!-- **c** -->",
    ]) {
      expect(texts()).toContain(kept);
    }
  });

  it("changes nothing in source mode", () => {
    const doc = "# Title\n\n**b** [a](b)";
    const { texts, q } = edit(doc, 0, { mode: "source" });
    expect(texts()).toEqual(["# Title", "", "**b** [a](b)"]);
    expect(q(".cm-lp-h1")).toHaveLength(0);
  });
});

describe("widgets", () => {
  it("draws a bullet for a bullet mark and leaves an ordered number as text", () => {
    const doc = "- one\n- two\n1. first\n2. second";
    const { texts, q, moveTo } = edit(doc, doc.length);
    expect(texts()).toEqual(["• one", "• two", "1. first", "2. second"]);
    expect(q(".cm-lp-bullet")).toHaveLength(2);
    moveTo(0);
    expect(texts()[0]).toBe("- one");
    expect(q(".cm-lp-bullet")).toHaveLength(1);
  });

  it("draws a quote bar for each quote mark and shows the mark on the cursor line", () => {
    const doc = "> one\n\n> > two";
    const { texts, q, moveTo } = edit(doc, 7);
    expect(q(".cm-lp-quote-bar")).toHaveLength(1);
    expect(texts()[0]).toBe(" one");
    moveTo(0);
    expect(texts()[0]).toBe("> one");
    expect(q(".cm-lp-quote-bar")).toHaveLength(2);
  });

  it("draws a task as a box and its label, and the source on the cursor line", () => {
    const doc = "- [ ] open\n- [X] shut\n\nend";
    const { handle, view, q, texts, moveTo } = edit(doc, doc.length);
    const boxes = [...q("input[type=checkbox]")] as HTMLInputElement[];
    expect(boxes.map((box) => box.checked)).toEqual([false, true]);
    expect(texts().slice(0, 2)).toEqual([" open", " shut"]);
    expect(q(".cm-lp-bullet")).toHaveLength(0);
    boxes[1]!.click();
    expect(handle.getText()).toBe("- [ ] open\n- [ ] shut\n\nend");
    undo(view);
    expect(handle.getText()).toBe(doc);
    moveTo(0);
    expect(texts()[0]).toBe("- [ ] open");
    expect(q("input[type=checkbox]")).toHaveLength(1);
  });

  it("gives heading lines the reading view's levels, on the cursor line too", () => {
    const doc = "# a\n## b\n### c\n#### d\n##### e\n###### f\n\ntext";
    const { lines, moveTo } = edit(doc, doc.length);
    const levels = () =>
      lines()
        .slice(0, 6)
        .map((line) => line.className.match(/cm-lp-h\d/)?.[0]);
    const expected = ["h1", "h2", "h3", "h4", "h5", "h6"].map((level) => `cm-lp-${level}`);
    expect(levels()).toEqual(expected);
    moveTo(0);
    expect(levels()).toEqual(expected);
    expect(lines()[6]!.className).not.toMatch(/cm-lp-h/);
  });

  it("labels inline code and marks a finished task's label", () => {
    const doc = "a `code` b\n\n- [x] done";
    const { q } = edit(doc, 0);
    expect(q(".cm-lp-inline-code")).toHaveLength(1);
    expect(q(".cm-lp-done")).toHaveLength(1);
  });

  it("dims table pipes and sets table lines in the monospace class", () => {
    const doc = "x\n\n| a | b |\n| - | - |\n| 1 | 2 |";
    const { q, texts } = edit(doc, 0);
    expect(q(".cm-lp-dim")).toHaveLength(7);
    expect(q(".cm-lp-mono")).toHaveLength(3);
    expect(texts().slice(2)).toEqual(["| a | b |", "| - | - |", "| 1 | 2 |"]);
  });
});

describe("fenced code", () => {
  const DOC = "intro\n\n```yaml\na: 1\n```\n## after";

  it("hides both fence lines off the block and labels the language", () => {
    const { texts, q } = edit(DOC, 0);
    expect(texts()).toEqual(["intro", "", "a: 1", "after"]);
    expect(q(".cm-lp-fence-label")[0]?.textContent).toBe("yaml");
  });

  it("shows both fences, without the label, while the cursor is in the block", () => {
    const { texts, q, moveTo } = edit(DOC, at(DOC, "a: 1"));
    expect(texts()).toEqual(["intro", "", "```yaml", "a: 1", "```", "after"]);
    expect(q(".cm-lp-fence-label")).toHaveLength(0);
    moveTo(0);
    expect(q(".cm-lp-fence-label")).toHaveLength(1);
  });

  it("puts the code ground on the body and not on the line after the block", () => {
    const { lines } = edit(DOC, 0);
    const [, , code, after] = lines();
    expect(code!.className).toContain("cm-lp-code-first");
    expect(code!.className).toContain("cm-lp-code-last");
    expect(after!.className).toContain("cm-lp-h2");
    expect(after!.className).not.toContain("cm-lp-code");
  });

  it("reveals the fence when the cursor reaches the line after it or before it", () => {
    const doc = "before\n~~~\nx\n~~~\nafter";
    const { texts, moveTo } = edit(doc, 0);
    expect(texts()).toEqual(["before", "x", "after"]);
    moveTo(at(doc, "after"));
    expect(texts()).toContain("~~~");
    moveTo(at(doc, "after") + 2);
    expect(texts()).not.toContain("~~~");
    moveTo(at(doc, "~~~") - 1);
    expect(texts()).toContain("~~~");
  });

  it("hides the one mark of an unterminated fence", () => {
    const doc = "x\n\n```\nopen\nstill";
    const { texts } = edit(doc, 0);
    expect(texts()).toEqual(["x", "", "open", "still"]);
  });

  it("hides the two adjacent lines of an empty fence", () => {
    const doc = "x\n\n```\n```\n\ny";
    const { texts, handle, moveTo } = edit(doc, 0);
    expect(texts().join("|")).not.toContain("`");
    expect(handle.getText()).toBe(doc);
    moveTo(at(doc, "```"));
    expect(texts().filter((line) => line === "```")).toHaveLength(2);
  });

  it("keeps the container prefix of a fence inside a list item", () => {
    const doc = "x\n\n- ```js\n  y\n  ```\n\nend";
    const { texts } = edit(doc, 0);
    expect(texts().join("\n")).not.toContain("```");
    expect(texts().join("\n")).toContain("y");
  });
});

describe("syntax tree rebuilds", () => {
  it("decorates text past the first parse once the background parse finishes", () => {
    const filler = Array.from({ length: 400 }, (_, i) => `plain line ${i + 1}`).join("\n");
    const doc = `${filler}\n\nsome **bold** text\n\n~~~yaml\na: 1\n~~~\nend`;
    const { view, lines, texts, handle } = edit(doc, 0);
    view.dispatch({ effects: EditorView.scrollIntoView(doc.indexOf("some **"), { y: "start" }) });
    const selection = view.state.selection;
    forceParsing(view, view.state.doc.length, 10_000);
    expect(view.state.selection.eq(selection)).toBe(true);
    expect(lines().find((line) => line.textContent?.includes("bold"))?.textContent).toBe(
      "some bold text",
    );
    expect(texts().join("\n")).not.toContain("~~~");
    expect(handle.getText()).toBe(doc);
  });
});

describe("Mod-click", () => {
  function click(view: EditorView, text: string, init: MouseEventInit): void {
    const doc = view.state.doc.toString();
    const { node } = view.domAtPos(doc.indexOf(text) + 1);
    node.dispatchEvent(
      new MouseEvent("mousedown", { bubbles: true, cancelable: true, button: 0, ...init }),
    );
  }

  it("follows a link on Ctrl or Meta, an autolink and a bare URL, but not an image", () => {
    const doc = "x\n\n[a](one.md) <https://two.example> https://three.example ![i](four.png)";
    const onFollowLink = vi.fn();
    const { view } = edit(doc, 0, { onFollowLink });
    click(view, "a](one", { ctrlKey: true });
    click(view, "two.example", { metaKey: true });
    click(view, "three.example", { ctrlKey: true });
    click(view, "four.png", { ctrlKey: true });
    expect(onFollowLink.mock.calls).toEqual([
      ["one.md"],
      ["https://two.example"],
      ["https://three.example"],
    ]);
  });

  it("hands over the destination the reading view parses, not its source spelling", () => {
    const doc = "x\n\n[s](<section 06.md>)\n\n[p](a\\(b\\).md)\n\n[u](plain.md)";
    const onFollowLink = vi.fn();
    const { view } = edit(doc, 0, { onFollowLink });
    click(view, "s](<section", { ctrlKey: true });
    click(view, "p](a", { ctrlKey: true });
    click(view, "u](plain", { ctrlKey: true });
    expect(onFollowLink.mock.calls).toEqual([["section 06.md"], ["a(b).md"], ["plain.md"]]);
  });

  it("ignores a Mod-click outside a link and a click with no handler", () => {
    const doc = "x\n\nplain [a](one.md)";
    const onFollowLink = vi.fn();
    const { view } = edit(doc, 0, { onFollowLink });
    click(view, "plain", { ctrlKey: true });
    expect(onFollowLink).not.toHaveBeenCalled();
    const bare = edit(doc, 0);
    expect(() => click(bare.view, "a](one", { ctrlKey: true })).not.toThrow();
  });
});

describe("mode", () => {
  it("marks the editor root only in live preview", () => {
    const { handle, parent } = edit("a", 0);
    const root = () => parent.querySelector(".cm-editor")!;
    expect(root().classList.contains("cm-live-preview")).toBe(true);
    handle.setMode("source");
    expect(root().classList.contains("cm-live-preview")).toBe(false);
    handle.setMode("preview");
    expect(root().classList.contains("cm-live-preview")).toBe(true);
  });

  it("opens in the mode it is given and reconfigures without a new view", () => {
    const { handle, view, parent } = edit("x\n\n**b**", 0, { mode: "source" });
    expect(handle.mode()).toBe("source");
    expect(parent.querySelector(".cm-lineNumbers")).not.toBeNull();
    handle.setMode("preview");
    expect(EditorView.findFromDOM(parent.querySelector<HTMLElement>(".cm-editor")!)).toBe(view);
    expect(view.contentDOM.textContent).toBe("xb");
  });
});
