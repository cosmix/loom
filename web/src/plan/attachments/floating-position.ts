import { useLayoutEffect, useRef, useState, type RefObject } from "react";

/** Space kept between the control and the viewport edge. */
const MARGIN = 8;
/** Space between the selection and the control. */
const GAP = 6;

type Size = { width: number; height: number };
type Placement = { top: number; left: number };

function clamp(value: number, low: number, high: number): number {
  // A viewport narrower than the control leaves no valid range; the low edge wins.
  return Math.max(low, Math.min(value, high));
}

/**
 * Where the floating control goes for a selection `rect` (viewport coordinates), or null when
 * the selection is entirely outside the viewport. The control sits below the selection, flips
 * above it when there is no room below, and is clamped horizontally into the viewport.
 */
export function placeControl(rect: DOMRect, size: Size, viewport: Size): Placement | null {
  if (
    rect.bottom < 0 ||
    rect.top > viewport.height ||
    rect.right < 0 ||
    rect.left > viewport.width
  ) {
    return null;
  }
  const left = clamp(rect.left, MARGIN, viewport.width - size.width - MARGIN);
  const below = rect.bottom + GAP;
  const above = rect.top - GAP - size.height;
  let top: number;
  if (below + size.height <= viewport.height - MARGIN) top = below;
  else if (above >= MARGIN) top = above;
  else top = clamp(below, MARGIN, viewport.height - size.height - MARGIN);
  return { top, left };
}

/**
 * Measures the control and places it next to `rect`. `placement` is null while the selection is
 * outside the viewport; the last measured size is kept so the control reappears in place.
 */
export function useFloatingPlacement(rect: DOMRect): {
  ref: RefObject<HTMLDivElement | null>;
  placement: Placement | null;
} {
  const ref = useRef<HTMLDivElement | null>(null);
  const [size, setSize] = useState<Size>({ width: 0, height: 0 });
  // Runs after every commit: the control's content (button, note, secondaries) can change size.
  useLayoutEffect(() => {
    const box = ref.current?.getBoundingClientRect();
    if (!box) return;
    setSize((prev) =>
      prev.width === box.width && prev.height === box.height
        ? prev
        : { width: box.width, height: box.height },
    );
  });
  const viewport = { width: window.innerWidth, height: window.innerHeight };
  return { ref, placement: placeControl(rect, size, viewport) };
}
