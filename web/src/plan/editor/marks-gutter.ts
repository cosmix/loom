// The plan-validation gutter of the source editor: a dot beside each marked line. It lives
// outside the mode compartment, so the markers show in live preview and in source.
import { RangeSet, StateEffect, StateField } from "@codemirror/state";
import { GutterMarker, gutter } from "@codemirror/view";

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

/** Replaces the marked 1-based lines. */
export const setMarks = StateEffect.define<readonly number[]>();

export const marksField = StateField.define<readonly number[]>({
  create: () => [],
  update(value, transaction) {
    for (const effect of transaction.effects) if (effect.is(setMarks)) return effect.value;
    return value;
  },
});

export const marksGutter = gutter({
  class: "cm-mark-gutter",
  markers(view) {
    const { doc } = view.state;
    const lines = [...new Set(view.state.field(marksField))]
      .filter((line) => Number.isInteger(line) && line >= 1 && line <= doc.lines)
      .sort((a, b) => a - b);
    return RangeSet.of(lines.map((line) => markedLine.range(doc.line(line).from)));
  },
});
