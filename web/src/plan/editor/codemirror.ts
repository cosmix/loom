// The lazy CodeMirror chunk: the only module that statically imports @codemirror/*.
// Every other file reaches it through loadCodeMirror() in ./load.
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { markdown } from "@codemirror/lang-markdown";
import { yaml } from "@codemirror/lang-yaml";
import { HighlightStyle, LanguageDescription, syntaxHighlighting } from "@codemirror/language";
import { MergeView, unifiedMergeView } from "@codemirror/merge";
import { search, searchKeymap, searchPanelOpen } from "@codemirror/search";
import { EditorState, RangeSet, StateEffect, StateField, type Extension } from "@codemirror/state";
import { EditorView, GutterMarker, gutter, keymap, lineNumbers } from "@codemirror/view";
import { tags } from "@lezer/highlight";

import { themeSpec } from "./theme";

type Change = (text: string) => void;

/** 1-based inclusive document lines. */
export type LineSpan = { start: number; end: number };

export interface EditorHandle {
  destroy(): void;
  getText(): string;
  setText(text: string): void;
  searchOpen(): boolean;
  /** Moves the cursor to the start of a 1-based line (clamped) and scrolls it to the centre. */
  scrollToLine(line: number): void;
  /**
   * The 1-based inclusive document lines a DOM range covers, or null when the range is not
   * inside an editor this handle owns (for a merge view, only the editable right side).
   */
  rangeLines(range: Range): LineSpan | null;
}

export interface SourceEditorHandle extends EditorHandle {
  /** Marks 1-based lines in the gutter; lines outside the document are ignored. */
  setGutterMarks(lines: readonly number[]): void;
}

export interface SourceEditorOptions {
  doc: string;
  readOnly?: boolean;
  onChange: Change;
  onSave: () => void;
}

export interface MergeViewOptions {
  left: string;
  right: string;
  onChange?: Change;
  /** Mod-s on the editable side; the browser's save dialog never opens. */
  onSave?: () => void;
}

export interface DiffViewOptions {
  before: string;
  after: string;
  unified?: boolean;
}

/** Width below which a diff falls back to the unified layout. */
export const UNIFIED_BELOW_PX = 900;

const yamlDescription = LanguageDescription.of({
  name: "yaml",
  alias: ["yml"],
  support: yaml(),
});

/** Resolves a Markdown fence info string to a language; `yaml` and `yml` map to lang-yaml. */
export function codeLanguages(info: string): LanguageDescription | null {
  const tag = info.trim().toLowerCase();
  return yamlDescription.alias.includes(tag) || tag === yamlDescription.name
    ? yamlDescription
    : null;
}

const theme = EditorView.theme(themeSpec);

// Colours are design-token variables only, so both colour schemes follow the page.
const highlightStyle = HighlightStyle.define([
  { tag: tags.heading, fontWeight: "bold", color: "var(--foreground)" },
  { tag: tags.strong, fontWeight: "bold" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.strikethrough, textDecoration: "line-through" },
  { tag: tags.link, color: "var(--chart-1)", textDecoration: "underline" },
  { tag: tags.url, color: "var(--chart-1)" },
  { tag: tags.monospace, color: "var(--chart-2)" },
  { tag: [tags.string, tags.special(tags.string)], color: "var(--chart-2)" },
  { tag: [tags.number, tags.integer, tags.float], color: "var(--chart-4)" },
  { tag: [tags.atom, tags.bool, tags.null], color: "var(--chart-4)" },
  { tag: [tags.keyword, tags.operatorKeyword], color: "var(--chart-3)" },
  { tag: [tags.propertyName, tags.definition(tags.propertyName)], color: "var(--chart-1)" },
  { tag: [tags.comment, tags.quote], color: "var(--muted-foreground)", fontStyle: "italic" },
  {
    tag: [tags.meta, tags.processingInstruction, tags.punctuation, tags.contentSeparator],
    color: "var(--muted-foreground)",
  },
]);

const baseExtensions = (): Extension[] => [
  lineNumbers(),
  history(),
  search({ top: true }),
  syntaxHighlighting(highlightStyle),
  markdown({ codeLanguages }),
  EditorView.lineWrapping,
  keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap, indentWithTab]),
  theme,
];

function changeListener(onChange: Change | undefined): Extension {
  return EditorView.updateListener.of((update) => {
    if (update.docChanged) onChange?.(update.state.doc.toString());
  });
}

function readOnlyExtensions(readOnly: boolean): Extension[] {
  return readOnly ? [EditorState.readOnly.of(true), EditorView.editable.of(false)] : [];
}

function replaceAll(view: EditorView, text: string): void {
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
}

function scrollToLine(view: EditorView, line: number): void {
  const { doc } = view.state;
  const target = Number.isFinite(line) ? Math.min(Math.max(1, Math.trunc(line)), doc.lines) : 1;
  const from = doc.line(target).from;
  view.dispatch({
    selection: { anchor: from },
    effects: EditorView.scrollIntoView(from, { y: "center" }),
  });
}

function rangeLines(views: readonly EditorView[], range: Range): LineSpan | null {
  const owner = views.find(
    (view) =>
      view.contentDOM.contains(range.startContainer) &&
      view.contentDOM.contains(range.endContainer),
  );
  if (!owner) return null;
  try {
    const { doc } = owner.state;
    const from = owner.posAtDOM(range.startContainer, range.startOffset);
    const to = owner.posAtDOM(range.endContainer, range.endOffset);
    const start = doc.lineAt(from).number;
    const last = doc.lineAt(to);
    // A selection that ends at a line start does not cover that line.
    const end = to === last.from && to > from ? last.number - 1 : last.number;
    return { start, end: Math.max(start, end) };
  } catch {
    // posAtDOM throws for nodes CodeMirror does not map (widgets, deleted-chunk DOM).
    return null;
  }
}

/** `others` are extra views whose selections this handle may attribute (diff sides). */
function viewHandle(
  view: EditorView,
  destroy: () => void,
  others: readonly EditorView[] = [],
): EditorHandle {
  return {
    destroy,
    getText: () => view.state.doc.toString(),
    setText: (text) => replaceAll(view, text),
    searchOpen: () => searchPanelOpen(view.state),
    scrollToLine: (line) => scrollToLine(view, line),
    rangeLines: (range) => rangeLines([view, ...others], range),
  };
}

/** Ahead of the default keymap so Ctrl-S / Cmd-S never reaches the browser's save. */
function saveKeymap(onSave: () => void): Extension {
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

class MarkedLine extends GutterMarker {
  override elementClass = "cm-marked-line";
  override toDOM(): Node {
    const dot = document.createElement("span");
    dot.className = "cm-line-mark";
    dot.textContent = "●";
    return dot;
  }
}

const markedLine = new MarkedLine();
const setMarks = StateEffect.define<readonly number[]>();

const marksField = StateField.define<readonly number[]>({
  create: () => [],
  update(value, transaction) {
    for (const effect of transaction.effects) if (effect.is(setMarks)) return effect.value;
    return value;
  },
});

const marksGutter = gutter({
  class: "cm-mark-gutter",
  markers(view) {
    const { doc } = view.state;
    const lines = [...new Set(view.state.field(marksField))]
      .filter((line) => Number.isInteger(line) && line >= 1 && line <= doc.lines)
      .sort((a, b) => a - b);
    return RangeSet.of(lines.map((line) => markedLine.range(doc.line(line).from)));
  },
});

export function createSourceEditor(
  parent: HTMLElement,
  options: SourceEditorOptions,
): SourceEditorHandle {
  const view = new EditorView({
    parent,
    state: EditorState.create({
      doc: options.doc,
      extensions: [
        saveKeymap(options.onSave),
        ...baseExtensions(),
        marksField,
        marksGutter,
        ...readOnlyExtensions(options.readOnly ?? false),
        changeListener(options.onChange),
      ],
    }),
  });
  return {
    ...viewHandle(view, () => view.destroy()),
    setGutterMarks: (lines) => view.dispatch({ effects: setMarks.of(lines) }),
  };
}

export function createMergeView(parent: HTMLElement, options: MergeViewOptions): EditorHandle {
  const merge = new MergeView({
    parent,
    a: {
      doc: options.left,
      extensions: [...baseExtensions(), ...readOnlyExtensions(true)],
    },
    b: {
      doc: options.right,
      extensions: [
        ...(options.onSave ? [saveKeymap(options.onSave)] : []),
        ...baseExtensions(),
        changeListener(options.onChange),
      ],
    },
    revertControls: "a-to-b",
    highlightChanges: true,
    gutter: true,
  });
  return viewHandle(merge.b, () => merge.destroy());
}

export function createDiffView(parent: HTMLElement, options: DiffViewOptions): EditorHandle {
  const unified = options.unified ?? parent.clientWidth < UNIFIED_BELOW_PX;
  const readOnly = readOnlyExtensions(true);
  if (unified) {
    const view = new EditorView({
      parent,
      state: EditorState.create({
        doc: options.after,
        extensions: [
          ...baseExtensions(),
          ...readOnly,
          unifiedMergeView({ original: options.before, mergeControls: false, gutter: true }),
        ],
      }),
    });
    return viewHandle(view, () => view.destroy());
  }
  const merge = new MergeView({
    parent,
    a: { doc: options.before, extensions: [...baseExtensions(), ...readOnly] },
    b: { doc: options.after, extensions: [...baseExtensions(), ...readOnly] },
    highlightChanges: true,
    gutter: true,
  });
  return viewHandle(merge.b, () => merge.destroy(), [merge.a]);
}
