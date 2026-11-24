// Part of the lazy CodeMirror chunk: reachable only through ./codemirror, never imported elsewhere.
import { getChunks } from "@codemirror/merge";
import { searchPanelOpen } from "@codemirror/search";
import { EditorView } from "@codemirror/view";

import type { EditorHandle, LineSpan } from "./codemirror";

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

// A chunk's B side is this view's document (the right editor of a side-by-side diff, or the
// unified view). A chunk that removes the last line starts one past the text, hence the clamp.
function scrollToFirstChange(view: EditorView): void {
  const first = getChunks(view.state)?.chunks[0];
  const { doc } = view.state;
  if (first) scrollToLine(view, doc.lineAt(Math.min(first.fromB, doc.length)).number);
}

// The unified diff's removed lines are a block widget (.cm-deletedChunk); posAtDOM returns its
// start, which would pin a selection in removed text on the kept line below it.
function inDeletedChunk(node: Node): boolean {
  const element = node instanceof Element ? node : node.parentElement;
  return element?.closest(".cm-deletedChunk") != null;
}

/** The view a DOM range lies in and its document positions, or null outside every view. */
function mapRange(
  views: readonly EditorView[],
  range: Range,
): { owner: EditorView; from: number; to: number } | null {
  const owner = views.find(
    (view) =>
      view.contentDOM.contains(range.startContainer) &&
      view.contentDOM.contains(range.endContainer),
  );
  if (!owner) return null;
  if (inDeletedChunk(range.startContainer) || inDeletedChunk(range.endContainer)) return null;
  try {
    const from = owner.posAtDOM(range.startContainer, range.startOffset);
    const to = owner.posAtDOM(range.endContainer, range.endOffset);
    return { owner, from, to };
  } catch {
    // posAtDOM throws for DOM nodes CodeMirror does not map (other widgets).
    return null;
  }
}

function rangeLines(views: readonly EditorView[], range: Range): LineSpan | null {
  const mapped = mapRange(views, range);
  if (!mapped) return null;
  const { owner, from, to } = mapped;
  const { doc } = owner.state;
  const start = doc.lineAt(from).number;
  const last = doc.lineAt(to);
  // A selection that ends at a line start does not cover that line.
  const end = Math.max(start, to === last.from && to > from ? last.number - 1 : last.number);
  if (views.length === 1) return { start, end };
  return { start, end, side: owner === views[0] ? "after" : "before" };
}

/**
 * `others` are extra views whose selections this handle may attribute (the diff's before
 * side). `destroy` is the full teardown, including `destroyWith`'s observer stop.
 */
export function viewHandle(
  view: EditorView,
  destroy: () => void,
  others: readonly EditorView[] = [],
): EditorHandle {
  return {
    destroy,
    getText: () => view.state.sliceDoc(),
    setText: (text) => replaceAll(view, text),
    searchOpen: () => searchPanelOpen(view.state),
    scrollToLine: (line) => scrollToLine(view, line),
    scrollToFirstChange: () => scrollToFirstChange(view),
    rangeLines: (range) => rangeLines([view, ...others], range),
    rangeText(range) {
      const mapped = mapRange([view, ...others], range);
      return mapped && mapped.owner.state.sliceDoc(mapped.from, mapped.to);
    },
  };
}
