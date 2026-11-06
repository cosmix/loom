// Toolbar and shortcut commands: each inserts or removes Markdown syntax around the selection
// as ONE transaction, so one undo reverses it. The formatting commands refuse a read-only state;
// the save shortcut only hands Ctrl-S / Cmd-S to `onSave`.
import { isolateHistory } from "@codemirror/commands";
import {
  EditorSelection,
  type ChangeSet,
  type ChangeSpec,
  type EditorState,
  type Extension,
  type Line,
  type SelectionRange,
  type StateCommand,
  type TransactionSpec,
} from "@codemirror/state";
import { keymap, type EditorView } from "@codemirror/view";

export type FormatCommand =
  | "bold"
  | "italic"
  | "code"
  | "link"
  | "heading"
  | "bulletList"
  | "orderedList"
  | "taskList"
  | "quote"
  | "codeBlock"
  | "table";

type Target = Parameters<StateCommand>[0];

// A boundary on both sides keeps a format step from merging into the typing around it.
function apply({ state, dispatch }: Target, spec: TransactionSpec): boolean {
  dispatch(
    state.update(spec, {
      scrollIntoView: true,
      userEvent: "input.format",
      annotations: isolateHistory.of("full"),
    }),
  );
  return true;
}

/** The selection with every position mapped after text inserted at it (a cursor moves past a new prefix). */
function mapAfter(state: EditorState, changes: ChangeSet): EditorSelection {
  const { ranges, mainIndex } = state.selection;
  return EditorSelection.create(
    ranges.map((r) =>
      EditorSelection.range(changes.mapPos(r.anchor, 1), changes.mapPos(r.head, 1)),
    ),
    mainIndex,
  );
}

/** The distinct document lines the selection covers; a range ending at a line start omits that line. */
function selectedLines(state: EditorState): Line[] {
  const seen = new Set<number>();
  const lines: Line[] = [];
  for (const range of state.selection.ranges) {
    const first = state.doc.lineAt(range.from);
    let last = state.doc.lineAt(range.to);
    if (!range.empty && range.to === last.from && last.number > first.number) {
      last = state.doc.line(last.number - 1);
    }
    for (let n = first.number; n <= last.number; n += 1) {
      if (!seen.has(n)) {
        seen.add(n);
        lines.push(state.doc.line(n));
      }
    }
  }
  return lines;
}

// ---- inline marks ------------------------------------------------------------------------

function run(state: EditorState, pos: number, step: 1 | -1, mark: string): number {
  let count = 0;
  for (;;) {
    const at = step > 0 ? pos + count : pos - count - 1;
    if (at < 0 || state.sliceDoc(at, at + 1) !== mark) return count;
    count += 1;
  }
}

// A single `*` or backtick is part of a longer run (`**`, double backticks): only an odd run
// on both sides is this mark's own pair.
function wrappedAround(state: EditorState, from: number, to: number, mark: string): boolean {
  const n = mark.length;
  if (from < n || state.sliceDoc(from - n, from) !== mark || state.sliceDoc(to, to + n) !== mark) {
    return false;
  }
  return n > 1 || Math.min(run(state, from, -1, mark), run(state, to, 1, mark)) % 2 === 1;
}

function toggleInline(state: EditorState, range: SelectionRange, mark: string) {
  const { from, to } = range;
  const n = mark.length;
  if (range.empty)
    return { changes: { from, insert: mark + mark }, range: EditorSelection.cursor(from + n) };
  if (wrappedAround(state, from, to, mark)) {
    const changes = [
      { from: from - n, to: from },
      { from: to, to: to + n },
    ];
    return { changes, range: EditorSelection.range(from - n, to - n) };
  }
  const text = state.sliceDoc(from, to);
  const inner = text.slice(n, text.length - n);
  const markedInside =
    text.length >= 2 * n &&
    text.startsWith(mark) &&
    text.endsWith(mark) &&
    (n > 1 || !(inner.startsWith(mark) || inner.endsWith(mark)));
  if (markedInside) {
    const changes = [
      { from, to: from + n },
      { from: to - n, to },
    ];
    return { changes, range: EditorSelection.range(from, to - 2 * n) };
  }
  const changes = [
    { from, insert: mark },
    { from: to, insert: mark },
  ];
  return { changes, range: EditorSelection.range(from + n, to + n) };
}

function inline(mark: string): StateCommand {
  return (target) => {
    const { state } = target;
    if (state.readOnly) return false;
    return apply(
      target,
      state.changeByRange((range) => toggleInline(state, range, mark)),
    );
  };
}

export const bold = inline("**");
export const italic = inline("*");
export const code = inline("`");

/** The selection becomes the text of `[text](url)` and the placeholder `url` is selected. */
export const link: StateCommand = (target) => {
  const { state } = target;
  if (state.readOnly) return false;
  return apply(
    target,
    state.changeByRange((range) => {
      const label = range.empty ? "text" : state.sliceDoc(range.from, range.to);
      // Document positions, not string offsets: a CRLF break is 2 characters but 1 position.
      const start = range.from + (range.empty ? label.length : range.to - range.from) + 3;
      return {
        changes: { from: range.from, to: range.to, insert: `[${label}](url)` },
        range: EditorSelection.range(start, start + 3),
      };
    }),
  );
};

// ---- line prefixes -----------------------------------------------------------------------

// Quote marks and indentation before a list marker belong to the container, not the marker.
const CONTAINER = String.raw`^((?:[ \t]*>)*[ \t]*)`;
const TASK_MARK = new RegExp(`${CONTAINER}[-*+] \\[[ xX]\\] `);
const BULLET_MARK = new RegExp(`${CONTAINER}[-*+] `);
const ORDERED_MARK = new RegExp(`${CONTAINER}\\d+[.)] `);
const QUOTE_MARK = /^([ \t]*)> ?/;
const HEADING_MARK = /^#{1,6} +/;

interface LineKind {
  /** The marker region at the start of `text`, split into the kept container and the marker. */
  find(text: string): { keep: number; end: number } | null;
  make(index: number): string;
}

function matcher(regex: RegExp, skip?: RegExp): LineKind["find"] {
  return (text) => {
    const match = regex.exec(text);
    if (!match || skip?.test(text)) return null;
    return { keep: match[1]!.length, end: match[0].length };
  };
}

const findTask = matcher(TASK_MARK);
const findBullet = matcher(BULLET_MARK, TASK_MARK);
const findOrdered = matcher(ORDERED_MARK);
// Any list marker, so a conversion replaces it instead of stacking a second one.
const findList: LineKind["find"] = (text) =>
  findTask(text) ?? findBullet(text) ?? findOrdered(text);

function containerLength(text: string): number {
  return new RegExp(CONTAINER).exec(text)![1]!.length;
}

function prefixEdit(line: Line, kind: LineKind, remove: boolean, index: number): ChangeSpec {
  const found = kind.find(line.text);
  if (remove && found) return { from: line.from + found.keep, to: line.from + found.end };
  const replaced = findList(line.text);
  const keep = replaced?.keep ?? containerLength(line.text);
  const to = replaced ? line.from + replaced.end : line.from + keep;
  return { from: line.from + keep, to, insert: kind.make(index) };
}

/** Blank lines take no prefix unless the selection is that single blank line. */
function targetLines(state: EditorState): Line[] {
  const lines = selectedLines(state);
  const filled = lines.filter((line) => line.text.trim() !== "");
  return filled.length > 0 ? filled : lines.slice(0, 1);
}

function togglePrefix(kind: LineKind): StateCommand {
  return (target) => {
    const { state } = target;
    if (state.readOnly) return false;
    const lines = targetLines(state);
    const remove = lines.every((line) => kind.find(line.text));
    const changes = state.changes(lines.map((line, i) => prefixEdit(line, kind, remove, i + 1)));
    return apply(target, { changes, selection: mapAfter(state, changes) });
  };
}

export const bulletList = togglePrefix({ find: findBullet, make: () => "- " });
export const orderedList = togglePrefix({ find: findOrdered, make: (index) => `${index}. ` });
export const taskList = togglePrefix({ find: findTask, make: () => "- [ ] " });
export const quote: StateCommand = (target) => {
  const { state } = target;
  if (state.readOnly) return false;
  const lines = targetLines(state);
  const remove = lines.every((line) => QUOTE_MARK.test(line.text));
  const changes = state.changes(
    lines.map((line) => {
      if (!remove) return { from: line.from, insert: "> " };
      const kept = QUOTE_MARK.exec(line.text)!;
      return { from: line.from + kept[1]!.length, to: line.from + kept[0].length };
    }),
  );
  return apply(target, { changes, selection: mapAfter(state, changes) });
};

/** Cycles the selected lines through `#`, `##`, `###` and no heading, by the first line's level. */
export const heading: StateCommand = (target) => {
  const { state } = target;
  if (state.readOnly) return false;
  const lines = targetLines(state);
  const markOf = (line: Line) => HEADING_MARK.exec(line.text.slice(containerLength(line.text)));
  const level = markOf(lines[0]!)?.[0].trim().length ?? 0;
  const next = level >= 3 ? 0 : level + 1;
  const prefix = next > 0 ? `${"#".repeat(next)} ` : "";
  // The marker goes after the container (`> `), where Markdown reads it.
  const changes = state.changes(
    lines.map((line) => {
      const from = line.from + containerLength(line.text);
      return { from, to: from + (markOf(line)?.[0].length ?? 0), insert: prefix };
    }),
  );
  return apply(target, { changes, selection: mapAfter(state, changes) });
};

// ---- blocks ------------------------------------------------------------------------------

const FENCE = "```";

/**
 * Wraps the selected lines in a fence; a cursor stays put, a selection keeps its lines selected.
 * Both fences take the first line's container prefix (`> `, indentation), so a quote stays whole.
 */
export const codeBlock: StateCommand = (target) => {
  const { state } = target;
  if (state.readOnly) return false;
  const lines = selectedLines(state);
  const first = lines[0]!;
  const last = lines[lines.length - 1]!;
  const { lineBreak } = state;
  const prefix = first.text.slice(0, containerLength(first.text));
  const at = first.from + prefix.length;
  // A break is one document position whatever its length in characters; the prefix is repeated.
  const opened = FENCE.length + 1 + prefix.length;
  const range = state.selection.main;
  const selection = range.empty
    ? EditorSelection.cursor(Math.max(range.head, at) + opened)
    : EditorSelection.range(at + FENCE.length + 1, last.to + opened);
  return apply(target, {
    changes: [
      { from: at, insert: FENCE + lineBreak + prefix },
      { from: last.to, insert: lineBreak + prefix + FENCE },
    ],
    selection,
  });
};

const TABLE_ROWS = ["| Column | Column |", "| ------ | ------ |", "|        |        |"];

/** Inserts a two-column table after the cursor's line (on it when blank) and selects a header cell. */
export const table: StateCommand = (target) => {
  const { state } = target;
  if (state.readOnly) return false;
  const line = state.doc.lineAt(state.selection.main.to);
  const blank = line.text.trim() === "";
  const above = line.number > 1 ? state.doc.line(line.number - 1).text.trim() : "";
  const below = line.number < state.doc.lines ? state.doc.line(line.number + 1).text.trim() : "";
  // A table runs until a blank line, so text directly below would become more rows.
  const breaks = blank ? (above === "" ? 0 : 1) : 2;
  const at = blank ? line.from : line.to;
  const { lineBreak } = state;
  const insert =
    lineBreak.repeat(breaks) + TABLE_ROWS.join(lineBreak) + (below === "" ? "" : lineBreak);
  const cell = at + breaks + 2;
  return apply(target, {
    changes: { from: at, to: blank ? line.to : at, insert },
    selection: EditorSelection.range(cell, cell + "Column".length),
  });
};

const COMMANDS: Record<FormatCommand, StateCommand> = {
  bold,
  italic,
  code,
  link,
  heading,
  bulletList,
  orderedList,
  taskList,
  quote,
  codeBlock,
  table,
};

export function formatCommand(name: FormatCommand): StateCommand {
  return COMMANDS[name];
}

// Kept here, not in codemirror.ts, so that file stays under 400 lines.
/** Ahead of the default keymap so Ctrl-S / Cmd-S never reaches the browser's save. */
export function saveKeymap(onSave: () => void): Extension {
  return keymap.of([
    {
      key: "Mod-s",
      run: () => {
        onSave();
        return true;
      },
      preventDefault: true,
    },
  ]);
}

// Every binding returns true, even when the command did nothing (read-only): a handled key
// gets preventDefault, so Mod-k never opens the browser's location bar and Mod-i never falls
// through to the default keymap's selectParentSyntax.
function bind(key: string, command: StateCommand) {
  return {
    key,
    preventDefault: true,
    run(view: EditorView): boolean {
      command(view);
      return true;
    },
  };
}

export function formatKeymap(): Extension {
  return keymap.of([bind("Mod-b", bold), bind("Mod-i", italic), bind("Mod-k", link)]);
}
