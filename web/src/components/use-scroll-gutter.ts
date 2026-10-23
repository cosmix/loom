import { useLayoutEffect, useRef } from "react";

/// Publishes the width of a scroll box's vertical scrollbar as `--gutter` on
/// the box, so its padding can give that width back and its content keeps the
/// same right edge as the unscrolled chrome above it. Re-measured when the box
/// or its content resizes, since the scrollbar comes and goes with overflow.
export function useScrollGutter<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  useLayoutEffect(() => {
    const box = ref.current;
    if (!box) return;
    const sync = () => box.style.setProperty("--gutter", `${box.offsetWidth - box.clientWidth}px`);
    sync();
    const observer = new ResizeObserver(sync);
    observer.observe(box);
    if (box.firstElementChild) observer.observe(box.firstElementChild);
    return () => observer.disconnect();
  }, []);
  return ref;
}
