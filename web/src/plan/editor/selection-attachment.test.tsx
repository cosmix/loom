import { act, cleanup, render } from "@testing-library/react";
import { createRef, type JSX, type RefObject } from "react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import type { AttachmentDraft } from "@/plan/attachments";

import type { EditorHandle } from "./codemirror";
import { useEditorSelectionAttachment } from "./selection-attachment";

beforeAll(() => {
  // jsdom has no layout; the attachment hook places itself from the selection's rectangle.
  Range.prototype.getBoundingClientRect = () => new DOMRect(0, 0, 10, 10);
  Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
});

afterEach(() => {
  cleanup();
  window.getSelection()?.removeAllRanges();
});

type Provenance = (
  selection: Selection,
  lines: { start: number; end: number },
  text?: string | null,
) => AttachmentDraft | null;

function Probe(props: {
  handle: RefObject<Partial<EditorHandle> | null>;
  provenance: Provenance;
}): JSX.Element {
  const container = createRef<HTMLDivElement>();
  useEditorSelectionAttachment(
    container,
    props.handle as RefObject<EditorHandle | null>,
    props.provenance,
  );
  return (
    <div ref={container}>
      <p data-testid="text">some bold text</p>
    </div>
  );
}

function selectText(view: ReturnType<typeof render>) {
  const range = document.createRange();
  range.selectNodeContents(view.getByTestId("text"));
  window.getSelection()!.removeAllRanges();
  window.getSelection()!.addRange(range);
  act(() => {
    document.dispatchEvent(new Event("selectionchange"));
  });
  return range;
}

describe("useEditorSelectionAttachment", () => {
  it("hands the lines and the rangeText result of the same range to the provenance", () => {
    const rangeLines = vi.fn(() => ({ start: 3, end: 4 }));
    const rangeText = vi.fn((_range: Range) => "some **bold** text");
    const provenance = vi.fn<Provenance>(() => null);
    const view = render(
      <Probe handle={{ current: { rangeLines, rangeText } }} provenance={provenance} />,
    );
    const range = selectText(view);

    expect(provenance).toHaveBeenCalled();
    const [, lines, text] = provenance.mock.calls.at(-1)!;
    expect(lines).toEqual({ start: 3, end: 4 });
    expect(text).toBe("some **bold** text");
    expect(rangeText.mock.calls.at(-1)![0].toString()).toBe(range.toString());
  });

  it("passes null text for a handle without rangeText", () => {
    const provenance = vi.fn<Provenance>(() => null);
    const handle = { current: { rangeLines: () => ({ start: 1, end: 1 }) } };
    const view = render(<Probe handle={handle} provenance={provenance} />);
    selectText(view);
    expect(provenance.mock.calls.at(-1)![2]).toBeNull();
  });

  it("makes no draft when the handle does not own the selection", () => {
    const provenance = vi.fn<Provenance>(() => null);
    const handle = { current: { rangeLines: () => null, rangeText: () => "x" } };
    const view = render(<Probe handle={handle} provenance={provenance} />);
    selectText(view);
    expect(provenance).not.toHaveBeenCalled();
  });
});
