import { history, undo } from "@codemirror/commands";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { afterEach, beforeAll, describe, expect, it } from "vitest";

import { formatCommand, type FormatCommand } from "./md-commands";

beforeAll(() => {
  // jsdom has no text layout, and CodeMirror measures through Range rectangles.
  Range.prototype.getBoundingClientRect = () => new DOMRect(0, 0, 0, 0);
  Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
});

const views: EditorView[] = [];

afterEach(() => {
  for (const view of views.splice(0)) view.destroy();
});

/** A view on `doc` with the selection `from`..`to` (a cursor when `to` is omitted). */
function open(doc: string, from = 0, to = from, readOnly = false, separator?: string) {
  const view = new EditorView({
    state: EditorState.create({
      doc,
      selection: { anchor: from, head: to },
      extensions: [
        history(),
        EditorState.readOnly.of(readOnly),
        ...(separator ? [EditorState.lineSeparator.of(separator)] : []),
      ],
    }),
  });
  views.push(view);
  const run = (name: FormatCommand) => formatCommand(name)(view);
  const text = () => view.state.sliceDoc();
  const range = () => [view.state.selection.main.from, view.state.selection.main.to];
  return { view, run, text, range };
}

const ALL: FormatCommand[] = [
  "bold",
  "italic",
  "code",
  "link",
  "heading",
  "bulletList",
  "orderedList",
  "taskList",
  "quote",
  "codeBlock",
  "table",
];

describe("inline marks", () => {
  it("wraps the selection and keeps it on the same text", () => {
    const { run, text, range } = open("a word here", 2, 6);
    run("bold");
    expect(text()).toBe("a **word** here");
    expect(range()).toEqual([4, 8]);
  });

  it("unwraps a selection that is already wrapped, and one that includes the marks", () => {
    const wrapped = open("a **word** here", 4, 8);
    wrapped.run("bold");
    expect(wrapped.text()).toBe("a word here");
    expect(wrapped.range()).toEqual([2, 6]);

    const inside = open("a **word** here", 2, 10);
    inside.run("bold");
    expect(inside.text()).toBe("a word here");
    expect(inside.range()).toEqual([2, 6]);
  });

  it("inserts the pair around an empty selection and puts the cursor between", () => {
    const { run, text, range } = open("ab", 1);
    run("code");
    expect(text()).toBe("a``b");
    expect(range()).toEqual([2, 2]);
  });

  it("keeps italic and bold apart on the same asterisk", () => {
    const bolded = open("**word**", 2, 6);
    bolded.run("italic");
    expect(bolded.text()).toBe("***word***");

    const italic = open("*word*", 1, 5);
    italic.run("italic");
    expect(italic.text()).toBe("word");

    const both = open("***word***", 3, 7);
    both.run("bold");
    expect(both.text()).toBe("*word*");
  });

  it("builds a link from the selection and selects the placeholder target", () => {
    const { run, text, range } = open("a word here", 2, 6);
    run("link");
    expect(text()).toBe("a [word](url) here");
    expect(range()).toEqual([9, 12]);
    const empty = open("", 0);
    empty.run("link");
    expect(empty.text()).toBe("[text](url)");
    expect(empty.range()).toEqual([7, 10]);
  });
});

describe("line prefixes", () => {
  it("cycles the heading level through three levels and back to none", () => {
    const { run, text } = open("title");
    const seen: string[] = [];
    for (let i = 0; i < 4; i += 1) {
      run("heading");
      seen.push(text());
    }
    expect(seen).toEqual(["# title", "## title", "### title", "title"]);
  });

  it("applies the first line's next level to every selected line", () => {
    const { run, text } = open("# a\nb\n#### c", 0, 12);
    run("heading");
    expect(text()).toBe("## a\n## b\n## c");
  });

  it("toggles a bullet on every selected line, skipping blank lines", () => {
    const { run, text } = open("a\n\nb", 0, 4);
    run("bulletList");
    expect(text()).toBe("- a\n\n- b");
    run("bulletList");
    expect(text()).toBe("a\n\nb");
  });

  it("numbers an ordered list in order and converts other list markers", () => {
    const { run, text } = open("a\n- b\n3. c", 0, 10);
    run("orderedList");
    expect(text()).toBe("1. a\n2. b\n3. c");
    run("orderedList");
    expect(text()).toBe("a\nb\nc");
  });

  it("makes task items and turns a plain bullet into one", () => {
    const { run, text } = open("- a\nb", 0, 5);
    run("taskList");
    expect(text()).toBe("- [ ] a\n- [ ] b");
    run("taskList");
    expect(text()).toBe("a\nb");
  });

  it("toggles a quote prefix, and keeps the quote when a list is added inside it", () => {
    const { run, text } = open("a\nb", 0, 3);
    run("quote");
    expect(text()).toBe("> a\n> b");
    run("bulletList");
    expect(text()).toBe("> - a\n> - b");
    run("bulletList");
    run("quote");
    expect(text()).toBe("a\nb");
  });

  it("moves a cursor at the line start past the new prefix", () => {
    const { run, text, range } = open("word", 0);
    run("bulletList");
    expect(text()).toBe("- word");
    expect(range()).toEqual([2, 2]);
  });

  it("leaves a selection that ends at a line start off that line", () => {
    const { run, text } = open("a\nb", 0, 2);
    run("quote");
    expect(text()).toBe("> a\nb");
  });
});

describe("blocks", () => {
  it("wraps the selected lines in a fence and keeps them selected", () => {
    const { run, text, range } = open("a\nb\nc", 0, 3);
    run("codeBlock");
    expect(text()).toBe("```\na\nb\n```\nc");
    expect(range()).toEqual([4, 7]);
  });

  it("wraps the line under a cursor and keeps the cursor on its character", () => {
    const { run, text, range } = open("abc", 1);
    run("codeBlock");
    expect(text()).toBe("```\nabc\n```");
    expect(range()).toEqual([5, 5]);
  });

  it("keeps a quote whole: both fences take the quote prefix", () => {
    const { run, text, range } = open("> a\n> b", 0, 7);
    run("codeBlock");
    expect(text()).toBe("> ```\n> a\n> b\n> ```");
    expect(range()).toEqual([6, 13]);
  });

  it("keeps the indentation of a list item's continuation lines", () => {
    const { run, text } = open("- x\n  a\n  b", 4, 11);
    run("codeBlock");
    expect(text()).toBe("- x\n  ```\n  a\n  b\n  ```");
  });

  it("opens a fence in a quote at a cursor and keeps it on its character", () => {
    const { run, text, range } = open("> abc", 3);
    run("codeBlock");
    expect(text()).toBe("> ```\n> abc\n> ```");
    expect(range()).toEqual([9, 9]);
  });

  it("inserts a table after the cursor's line, set apart by a blank line", () => {
    const { run, text, range } = open("para", 4);
    run("table");
    expect(text()).toBe("para\n\n| Column | Column |\n| ------ | ------ |\n|        |        |");
    expect(range()).toEqual([8, 14]);
  });

  it("puts a table on a blank line and keeps following text out of its rows", () => {
    const { run, text } = open("a\n\nnext", 2);
    run("table");
    expect(text()).toBe(
      "a\n\n| Column | Column |\n| ------ | ------ |\n|        |        |\n\nnext",
    );
  });

  it("starts a table in an empty document", () => {
    const { run, text, range } = open("");
    run("table");
    expect(text().startsWith("| Column | Column |")).toBe(true);
    expect(range()).toEqual([2, 8]);
  });
});

describe("heading in containers and blank lines", () => {
  it("puts the marker after a quote prefix", () => {
    const { run, text, range } = open("> note", 3);
    run("heading");
    expect(text()).toBe("> # note");
    expect(range()).toEqual([5, 5]);
    run("heading");
    expect(text()).toBe("> ## note");
  });

  it("cycles back to none inside a container", () => {
    const { run, text } = open("> ### note", 0, 10);
    run("heading");
    expect(text()).toBe("> note");
  });

  it("skips blank lines inside a selection", () => {
    const { run, text } = open("# a\n\nb", 0, 6);
    run("heading");
    expect(text()).toBe("## a\n\n## b");
  });

  it("still marks a lone blank line", () => {
    const { run, text } = open("a\n\nb", 2);
    run("heading");
    expect(text()).toBe("a\n# \nb");
  });
});

describe.each([
  ["CRLF", "\r\n"],
  ["CR", "\r"],
])("%s documents", (_name, br) => {
  /** Every break in the document is the document's own. */
  const onlyBreaks = (text: string) => expect(text.replaceAll(br, "").match(/[\r\n]/)).toBeNull();

  it("wraps lines in a fence with the document's break", () => {
    const { run, text, range } = open(`a${br}b`, 0, 3, false, br);
    run("codeBlock");
    expect(text()).toBe(`\`\`\`${br}a${br}b${br}\`\`\``);
    onlyBreaks(text());
    expect(range()).toEqual([4, 7]);
  });

  it("keeps the quote prefix on both fences with the document's break", () => {
    const { run, text, range } = open(`> a${br}> b`, 0, 5, false, br);
    run("codeBlock");
    expect(text()).toBe(`> \`\`\`${br}> a${br}> b${br}> \`\`\``);
    onlyBreaks(text());
    expect(range()).toEqual([6, 13]);
  });

  it("opens an empty fence at a cursor and keeps the cursor inside", () => {
    const { run, text, range } = open(`a${br}b`, 0, 0, false, br);
    run("codeBlock");
    expect(text()).toBe(`\`\`\`${br}a${br}\`\`\`${br}b`);
    onlyBreaks(text());
    expect(range()).toEqual([4, 4]);
  });

  it("inserts a table with the document's break and selects the header cell", () => {
    const { run, text, range, view } = open(`a${br}b`, 0, 0, false, br);
    run("table");
    onlyBreaks(text());
    expect(text()).toBe(
      `a${br}${br}| Column | Column |${br}| ------ | ------ |${br}|        |        |${br}${br}b`,
    );
    const [from, to] = range();
    expect(view.state.sliceDoc(from, to)).toBe("Column");
  });

  it("places the url selection of a multi-line link after the label", () => {
    const { run, text, range, view } = open(`a${br}b`, 0, 3, false, br);
    run("link");
    expect(text()).toBe(`[a${br}b](url)`);
    onlyBreaks(text());
    const [from, to] = range();
    expect(view.state.sliceDoc(from, to)).toBe("url");
  });
});

describe("transactions", () => {
  it("every command is one undo step", () => {
    for (const name of ALL) {
      const { view, run, text } = open("one\ntwo", 0, 7);
      run(name);
      expect(text(), name).not.toBe("one\ntwo");
      undo(view);
      expect(text(), name).toBe("one\ntwo");
    }
  });

  it("every command refuses a read-only state without dispatching", () => {
    for (const name of ALL) {
      const { view, run, text } = open("one\ntwo", 0, 7, true);
      let dispatched = 0;
      const result = formatCommand(name)({
        state: view.state,
        dispatch: () => {
          dispatched += 1;
        },
      });
      expect(result, name).toBe(false);
      expect(dispatched, name).toBe(0);
      run(name);
      expect(text(), name).toBe("one\ntwo");
    }
  });
});
