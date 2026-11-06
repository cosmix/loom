// Live preview: decorations and widgets drawn over the Markdown source. Nothing here changes
// the document except a task-box click, so a save writes the bytes the editor holds.
import { syntaxTree } from "@codemirror/language";
import { type EditorState, type Extension, type Range, StateField } from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  ViewPlugin,
  type ViewUpdate,
  WidgetType,
} from "@codemirror/view";

export type EditorMode = "preview" | "source";

export interface LivePreviewOptions {
  /** Mod-click on a link; the raw href from the Markdown source. */
  onFollowLink?(href: string): void;
}

// Node types come from the tree the Markdown language already parsed; @lezer/common is not a
// direct dependency, so its types are read off `syntaxTree`.
type Tree = ReturnType<typeof syntaxTree>;
type NodeRef = Parameters<NonNullable<Parameters<Tree["iterate"]>[0]["enter"]>>[0];
type SyntaxNode = NodeRef["node"];

const hide = Decoration.replace({});
const dim = Decoration.mark({ class: "cm-lp-dim" });
const inlineCode = Decoration.mark({ class: "cm-lp-inline-code" });
const done = Decoration.mark({ class: "cm-lp-done" });
const tableLine = Decoration.line({ class: "cm-lp-mono" });
const headingLine = [1, 2, 3, 4, 5, 6].map((n) => Decoration.line({ class: `cm-lp-h${n}` }));
const codeLine = [false, true].flatMap((first) =>
  [false, true].map((last) =>
    Decoration.line({
      class: `cm-lp-code${first ? " cm-lp-code-first" : ""}${last ? " cm-lp-code-last" : ""}`,
    }),
  ),
);

// A static span: the bullet, the quote bar and the fence's language label.
class SpanWidget extends WidgetType {
  readonly className: string;
  readonly text: string;
  constructor(className: string, text = "") {
    super();
    this.className = className;
    this.text = text;
  }
  override eq(other: WidgetType): boolean {
    return (
      other instanceof SpanWidget && other.className === this.className && other.text === this.text
    );
  }
  override toDOM(): HTMLElement {
    const span = document.createElement("span");
    span.className = this.className;
    span.textContent = this.text;
    return span;
  }
}

const bullet = Decoration.replace({ widget: new SpanWidget("cm-lp-bullet", "•") });
const quoteBar = Decoration.replace({ widget: new SpanWidget("cm-lp-quote-bar") });

// Widget DOM ignores editor events by default, so the box carries its own listener. It
// cancels the browser's own toggle: the redrawn widget shows the document's state, which
// stays unchanged when the editor is read-only.
class CheckboxWidget extends WidgetType {
  readonly checked: boolean;
  constructor(checked: boolean) {
    super();
    this.checked = checked;
  }
  override eq(other: WidgetType): boolean {
    return other instanceof CheckboxWidget && other.checked === this.checked;
  }
  override toDOM(view: EditorView): HTMLElement {
    const input = document.createElement("input");
    input.type = "checkbox";
    input.className = "cm-lp-check";
    input.checked = this.checked;
    input.setAttribute("aria-label", "Task");
    input.addEventListener("click", (event) => {
      event.preventDefault();
      toggleTask(view, input);
    });
    return input;
  }
}

function toggleTask(view: EditorView, input: HTMLElement): void {
  if (view.state.readOnly) return;
  const from = view.posAtDOM(input);
  const marker = view.state.sliceDoc(from, from + 3);
  if (!/^\[[ xX]\]$/.test(marker)) return;
  view.dispatch({
    changes: { from: from + 1, to: from + 2, insert: marker[1] === " " ? "x" : " " },
    userEvent: "input.toggle",
  });
}

/** A selection range touches [from, to] when they overlap, inclusive at both ends. */
function touches(state: EditorState, from: number, to: number): boolean {
  return state.selection.ranges.some((range) => range.from <= to && range.to >= from);
}

interface Ctx {
  state: EditorState;
  tree: Tree;
  out: Range<Decoration>[];
  /** The visible range being walked; nodes that straddle it are decorated only inside it. */
  from: number;
  to: number;
}

/** Hides one run of a line unless a selection range touches that line. Runs never span lines. */
function hideRun(ctx: Ctx, from: number, to: number, decoration: Decoration = hide): void {
  const line = ctx.state.doc.lineAt(from);
  if (to > line.to || touches(ctx.state, line.from, line.to)) return;
  ctx.out.push(decoration.range(from, to));
}

const spaceAt = (state: EditorState, pos: number): number =>
  state.sliceDoc(pos, pos + 1) === " " ? 1 : 0;

/** Puts `pick(lineNumber)` on each line of [from, to] inside the visible range. */
function lineClasses(
  ctx: Ctx,
  from: number,
  to: number,
  pick: (lineNumber: number) => Decoration,
): void {
  const { doc } = ctx.state;
  const end = Math.min(to, ctx.to);
  for (let pos = Math.max(from, ctx.from); pos <= end;) {
    const line = doc.lineAt(pos);
    ctx.out.push(pick(line.number).range(line.from));
    pos = line.to + 1;
  }
}

interface FenceParts {
  open: SyntaxNode | null;
  close: SyntaxNode | null;
  info: SyntaxNode | null;
}

function fenceParts(node: SyntaxNode): FenceParts {
  const marks = node.getChildren("CodeMark");
  return { open: marks[0] ?? null, close: marks[1] ?? null, info: node.getChild("CodeInfo") };
}

// The block is widened by a character at each end, so Backspace at the start of the line after
// a hidden closing fence, or Delete at the end of the line before an opening one, reveals the
// fence before it can join the next line unseen.
const fenceShown = (state: EditorState, node: NodeRef): boolean =>
  touches(state, node.from - 1, node.to + 1);

// Inline decorations come from a plugin: none of them replaces a line break.
type Handler = (ctx: Ctx, node: NodeRef) => boolean | void;

function headingHandler(level: number): Handler {
  return (ctx, node) => lineClasses(ctx, node.from, node.from, () => headingLine[level - 1]!);
}

function setextHeading(level: number): Handler {
  return (ctx, node) => {
    const { doc } = ctx.state;
    const first = doc.lineAt(node.from).number;
    const underline = node.node.getChild("HeaderMark");
    const last = underline ? doc.lineAt(underline.from).number - 1 : first;
    lineClasses(ctx, node.from, doc.line(Math.max(first, last)).to, () => headingLine[level - 1]!);
  };
}

function headerMark(ctx: Ctx, node: NodeRef): void {
  const heading = node.node.parent;
  if (!heading?.name.startsWith("ATXHeading")) return;
  const { state } = ctx;
  if (node.from === heading.from) return hideRun(ctx, node.from, node.to + spaceAt(state, node.to));
  const hasSpace = node.from > 0 && state.sliceDoc(node.from - 1, node.from) === " ";
  hideRun(ctx, node.from - (hasSpace ? 1 : 0), node.to);
}

const hideMark: Handler = (ctx, node) => hideRun(ctx, node.from, node.to);

function codeMark(ctx: Ctx, node: NodeRef): void {
  if (node.node.parent?.name === "InlineCode") hideRun(ctx, node.from, node.to);
}

// `[label](url "title")` reads as `label`: the opening `[` and the run from the closing `]`
// to the end of the node go. Reference links, images and autolinks hide nothing.
function linkHandler(ctx: Ctx, node: NodeRef): void {
  if (!node.node.getChild("URL")) return;
  const [open, close] = node.node.getChildren("LinkMark");
  if (!open || !close || open.from !== node.from) return;
  hideRun(ctx, open.from, open.to);
  hideRun(ctx, close.from, node.to);
}

function listMark(ctx: Ctx, node: NodeRef): void {
  const item = node.node.parent;
  if (item?.parent?.name !== "BulletList") return;
  // The box stands in for a task's bullet; an ordered mark keeps its number.
  if (item.getChild("Task")) hideRun(ctx, node.from, node.to + spaceAt(ctx.state, node.to));
  else hideRun(ctx, node.from, node.to, bullet);
}

// The item label (the Task node past its marker) stays visible with its own decorations.
function taskMarker(ctx: Ctx, node: NodeRef): void {
  const checked = ctx.state.sliceDoc(node.from + 1, node.from + 2).toLowerCase() === "x";
  const task = node.node.parent;
  if (checked && task && task.to > node.to) ctx.out.push(done.range(node.to, task.to));
  hideRun(ctx, node.from, node.to, Decoration.replace({ widget: new CheckboxWidget(checked) }));
}

function fencedCode(ctx: Ctx, node: NodeRef): false {
  const { doc } = ctx.state;
  const first = doc.lineAt(node.from).number;
  let last = doc.lineAt(node.to).number;
  // Hidden fences are swallowed into their neighbours: the closing one into the last body
  // line, the opening one into the first body line, which then shares the first line.
  if (!fenceShown(ctx.state, node)) {
    if (fenceParts(node.node).close) last -= 1;
    if (last === first + 1) last = first;
  }
  lineClasses(
    ctx,
    node.from,
    node.to,
    (n) => codeLine[(n === first ? 2 : 0) + (n === last ? 1 : 0)]!,
  );
  return false;
}

function tableHandler(ctx: Ctx, node: NodeRef): false {
  lineClasses(ctx, node.from, node.to, () => tableLine);
  ctx.tree.iterate({
    from: Math.max(node.from, ctx.from),
    to: Math.min(node.to, ctx.to),
    enter(child) {
      if (child.name === "TableDelimiter") ctx.out.push(dim.range(child.from, child.to));
    },
  });
  return false;
}

// HTML blocks and comments (the `<!-- loom METADATA -->` markers included) are never hidden;
// the walk does not enter them, nor mounted sub-trees below fences and tables.
const HANDLERS: Record<string, Handler> = {
  ...Object.fromEntries([1, 2, 3, 4, 5, 6].map((n) => [`ATXHeading${n}`, headingHandler(n)])),
  SetextHeading1: setextHeading(1),
  SetextHeading2: setextHeading(2),
  HeaderMark: headerMark,
  EmphasisMark: hideMark,
  StrikethroughMark: hideMark,
  CodeMark: codeMark,
  InlineCode: (ctx, node) => void ctx.out.push(inlineCode.range(node.from, node.to)),
  Link: linkHandler,
  ListMark: listMark,
  QuoteMark: (ctx, node) => hideRun(ctx, node.from, node.to, quoteBar),
  TaskMarker: taskMarker,
  FencedCode: fencedCode,
  Table: tableHandler,
  HTMLBlock: () => false,
  CommentBlock: () => false,
  Comment: () => false,
};

function buildInline(view: EditorView): DecorationSet {
  const { state } = view;
  const tree = syntaxTree(state);
  const out: Range<Decoration>[] = [];
  for (const { from, to } of view.visibleRanges) {
    const ctx: Ctx = { state, tree, out, from, to };
    tree.iterate({ from, to, enter: (node) => HANDLERS[node.name]?.(ctx, node) ?? true });
  }
  return Decoration.set(out, true);
}

// Rebuilt when the document, selection or viewport changes AND when the syntax tree does: the
// first parse covers only the start of the document, and the rest arrives in idle slices that
// change none of the other three.
const rebuilds = (update: ViewUpdate): boolean =>
  update.docChanged ||
  update.selectionSet ||
  update.viewportChanged ||
  syntaxTree(update.state) !== syntaxTree(update.startState);

const inlinePreview = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = buildInline(view);
    }
    update(update: ViewUpdate): void {
      if (rebuilds(update)) this.decorations = buildInline(update.view);
    }
  },
  { decorations: (plugin) => plugin.decorations },
);

// Fence lines come from a state field: a replaced range there may span a line break.
function hideFence(state: EditorState, node: NodeRef, out: Range<Decoration>[]): void {
  const { open, close, info } = fenceParts(node.node);
  if (!open || fenceShown(state, node)) return;
  const { doc } = state;
  // The opening fence goes from its mark through the line break, so any indentation or `>`
  // before it stays. The closing one goes from the previous line's end, which leaves no empty
  // line and lets the next line keep its own line decorations.
  const openEnd = Math.min(doc.lineAt(open.from).to + 1, doc.length);
  out.push(hide.range(open.from, openEnd));
  if (info) {
    const label = new SpanWidget("cm-lp-fence-label", state.sliceDoc(info.from, info.to).trim());
    out.push(Decoration.widget({ widget: label, side: -1 }).range(open.from));
  }
  if (!close) return;
  const closeLine = doc.lineAt(close.from);
  const before = closeLine.number > 1 ? doc.line(closeLine.number - 1).to : closeLine.from;
  out.push(hide.range(Math.max(before, openEnd), closeLine.to));
}

const SKIP_BLOCKS = /^(Paragraph|Table|HTMLBlock|CommentBlock|ATXHeading|SetextHeading)/;

function buildFences(state: EditorState): DecorationSet {
  const out: Range<Decoration>[] = [];
  syntaxTree(state).iterate({
    enter(node) {
      if (node.name === "FencedCode") {
        hideFence(state, node, out);
        return false;
      }
      return !SKIP_BLOCKS.test(node.name);
    },
  });
  // Sorted by the set: an empty fence body makes the two replaced lines adjacent.
  return Decoration.set(out, true);
}

const fenceField = StateField.define<DecorationSet>({
  create: buildFences,
  update(value, transaction) {
    const changed =
      transaction.docChanged ||
      !transaction.newSelection.eq(transaction.startState.selection) ||
      syntaxTree(transaction.state) !== syntaxTree(transaction.startState);
    return changed ? buildFences(transaction.state) : value;
  },
  provide: (field) => EditorView.decorations.from(field),
});

// A link's target: a Link or Autolink's URL child, or a bare URL node.
function urlOf(node: SyntaxNode): SyntaxNode | null {
  if (node.name === "URL") return node.parent?.name === "Image" ? null : node;
  return node.name === "Link" || node.name === "Autolink" ? node.getChild("URL") : null;
}

// A destination as the reading view parses it: no wrapping `<...>`, no escaping backslashes.
const ESCAPED = /\\([!-/:-@[-`{-~])/g;
const destination = (raw: string): string =>
  (raw.startsWith("<") && raw.endsWith(">") ? raw.slice(1, -1) : raw).replace(ESCAPED, "$1");

function hrefAt(view: EditorView, target: Node): string | null {
  let pos: number;
  try {
    pos = view.posAtDOM(target);
  } catch {
    return null;
  }
  const tree = syntaxTree(view.state);
  for (const side of [1, -1] as const) {
    for (let node: SyntaxNode | null = tree.resolveInner(pos, side); node; node = node.parent) {
      if (node.name === "Image") return null;
      const url = urlOf(node);
      if (url) return destination(view.state.sliceDoc(url.from, url.to));
    }
  }
  return null;
}

function followLinks(onFollowLink: ((href: string) => void) | undefined): Extension {
  return EditorView.domEventHandlers({
    mousedown(event, view) {
      const modifier = event.metaKey || event.ctrlKey;
      if (!onFollowLink || !modifier || event.button !== 0) return false;
      if (!(event.target instanceof Node)) return false;
      const href = hrefAt(view, event.target);
      if (href === null) return false;
      onFollowLink(href);
      return true;
    },
  });
}

/** Marker hiding, widgets, block styling and Mod-click on links; adds no change to the text. */
export function livePreview(options: LivePreviewOptions): Extension {
  return [inlinePreview, fenceField, followLinks(options.onFollowLink)];
}
