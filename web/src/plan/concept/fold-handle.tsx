import {
  useEffect,
  useLayoutEffect,
  useState,
  type CSSProperties,
  type JSX,
  type RefObject,
} from "react";

type Span = { top: number; height: number };

/** The run's vertical extent relative to its paragraph, from its first to its last line box. */
function measure(run: HTMLElement): Span | null {
  const paragraph = run.closest<HTMLElement>(".concept-paragraph");
  const rects = run.getClientRects();
  const first = rects[0];
  const last = rects[rects.length - 1];
  if (!paragraph || !first || !last) return null;
  const origin = paragraph.getBoundingClientRect().top;
  return { top: first.top - origin, height: Math.max(last.bottom - first.top, 0) };
}

function useSpan(target: RefObject<HTMLElement | null>): Span | null {
  const [span, setSpan] = useState<Span | null>(null);
  const update = () => {
    const run = target.current;
    const next = run ? measure(run) : null;
    setSpan((current) =>
      current?.top === next?.top && current?.height === next?.height ? current : next,
    );
  };
  // Every commit: a run's lines move when anything above or inside it changes.
  useLayoutEffect(update);
  useEffect(() => {
    if (typeof ResizeObserver === "undefined" || !target.current) return;
    const observer = new ResizeObserver(update);
    observer.observe(target.current);
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
 * Escape or the left arrow; the handle is for the pointer and assistive technology.
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
