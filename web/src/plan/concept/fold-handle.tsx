import {
  useEffect,
  useLayoutEffect,
  useState,
  type CSSProperties,
  type JSX,
  type RefObject,
} from "react";

/** The run's vertical extent in its paragraph, and the id of the run the handle folds. */
type Span = { top: number; height: number; runId: string };

/** The measurement origin: a run's top and height are relative to its paragraph. */
const PARAGRAPH = ".concept-paragraph";

function sameSpan(a: Span | null, b: Span | null): boolean {
  return a?.top === b?.top && a?.height === b?.height && a?.runId === b?.runId;
}

/** The run's vertical extent relative to its paragraph, from its first to its last line box. */
function measure(run: HTMLElement): Span | null {
  const paragraph = run.closest<HTMLElement>(PARAGRAPH);
  const rects = run.getClientRects();
  const first = rects[0];
  const last = rects[rects.length - 1];
  if (!paragraph || !first || !last) return null;
  const origin = paragraph.getBoundingClientRect().top;
  return { top: first.top - origin, height: Math.max(last.bottom - first.top, 0), runId: run.id };
}

function useSpan(target: RefObject<HTMLElement | null>): Span | null {
  const [span, setSpan] = useState<Span | null>(null);
  const update = () => {
    const run = target.current;
    const next = run ? measure(run) : null;
    setSpan((current) => (sameSpan(current, next) ? current : next));
  };
  // Every commit: a run's lines move when anything above or inside it changes.
  useLayoutEffect(update);
  useEffect(() => {
    const run = target.current;
    if (typeof ResizeObserver === "undefined" || !run) return;
    const observer = new ResizeObserver(update);
    observer.observe(run);
    // An inline run is a span with no box of its own, so observing it never fires. Its
    // paragraph is the measurement origin and resizes when a nested passage loads.
    const paragraph = run.closest(PARAGRAPH);
    if (paragraph) observer.observe(paragraph);
    window.addEventListener("resize", update);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", update);
    };
  });
  return span;
}

/**
 * The thin muted bracket in the left margin that spans an opened run and folds it. Nested
 * runs step further out with `depth`. Tab leaves the column, so the keyboard folds with
 * Escape or the left arrow; the handle is for the pointer and assistive technology. The
 * accessible name stays "Fold"; the run's own label is its description, so the handles of
 * several open runs are told apart.
 */
export function FoldHandle({
  target,
  depth,
  onFold,
}: {
  target: RefObject<HTMLElement | null>;
  depth: number;
  onFold: () => void;
}): JSX.Element {
  const span = useSpan(target);
  const style = {
    "--depth": depth,
    ...(span ? { top: span.top, height: span.height } : {}),
  } as CSSProperties;
  return (
    <button
      type="button"
      tabIndex={-1}
      aria-label="Fold"
      aria-describedby={span?.runId || undefined}
      className="concept-fold-handle"
      data-depth={depth}
      style={style}
      onClick={(event) => {
        event.stopPropagation();
        onFold();
      }}
    />
  );
}
