import { useLayoutEffect, useRef, useState, type RefObject } from "react";

import type { Turn } from "@/plan/api/schema-chat";

/** How near its end, in px, the transcript counts as following it. */
const NEAR_END = 32;
/** The reader's own input on the transcript, and how long after it a scroll counts as theirs. */
const READER_INPUT = ["wheel", "touchmove", "pointerdown", "keydown", "focusin"] as const;
const READER_MS = 1000;

/** Where the scroller was at the last scroll event or pin: its offset and the sizes it had. */
type Seen = { top: number; height: number; client: number };

function record(node: HTMLDivElement, seen: RefObject<Seen>): void {
  seen.current = { top: node.scrollTop, height: node.scrollHeight, client: node.clientHeight };
}

/** Scrolls to the end and records where that is, so the scroll event it causes is no move away. */
function pin(scroller: RefObject<HTMLDivElement | null>, seen: RefObject<Seen>): void {
  const node = scroller.current;
  if (!node) return;
  node.scrollTop = node.scrollHeight;
  record(node, seen);
}

/**
 * Keeps the transcript on its end while the reader is there, and says when activity arrived
 * below a reader who scrolled away.
 *
 * The end moves without the reader: a reply grows the content, and a growing composer or a
 * shorter window takes height from the scroller. A scroll event then finds the end further away
 * (the echo of a pin, or the browser clamping `scrollTop`, delivered after the layout changed
 * again), and taking that for the reader leaving would stop the transcript with its last lines
 * under the fold. So only a scroll towards the start leaves, and only when it is the reader's: just
 * after their own input, or with the layout unchanged since the last record (so no layout change
 * moved it). The second case covers scrolls that fire no input event on the scroller: find in
 * page, screen-reader browse mode, middle-click autoscroll, a scrollbar held before the drag.
 */
export function useFollow(turns: Turn[], previous: unknown) {
  const scroller = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const following = useRef(true);
  const seen = useRef<Seen>({ top: 0, height: 0, client: 0 });
  const readerAt = useRef(Number.NEGATIVE_INFINITY);
  const [activity, setActivity] = useState(false);
  const follow = () => {
    pin(scroller, seen);
    following.current = true;
    setActivity(false);
  };
  const onScroll = () => {
    const node = scroller.current;
    if (!node) return;
    const atEnd = node.scrollHeight - node.clientHeight - node.scrollTop <= NEAR_END;
    const last = seen.current;
    const byInput = performance.now() - readerAt.current < READER_MS;
    const sameLayout = node.scrollHeight === last.height && node.clientHeight === last.client;
    if (atEnd) following.current = true;
    else if ((byInput || sameLayout) && node.scrollTop < last.top) following.current = false;
    record(node, seen);
    if (following.current) setActivity(false);
  };
  useLayoutEffect(() => {
    if (following.current) pin(scroller, seen);
    else setActivity(true);
  }, [turns, previous]);
  useLayoutEffect(() => {
    if (typeof ResizeObserver === "undefined" || !content.current || !scroller.current) return;
    const observer = new ResizeObserver(() => {
      if (following.current) pin(scroller, seen);
    });
    observer.observe(content.current);
    observer.observe(scroller.current);
    return () => observer.disconnect();
  }, []);
  useLayoutEffect(() => {
    const node = scroller.current;
    if (!node) return;
    const mark = () => {
      readerAt.current = performance.now();
    };
    for (const type of READER_INPUT) node.addEventListener(type, mark, { passive: true });
    return () => {
      for (const type of READER_INPUT) node.removeEventListener(type, mark);
    };
  }, []);
  return { scroller, content, activity, follow, onScroll };
}
