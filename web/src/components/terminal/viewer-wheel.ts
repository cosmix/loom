import type { Terminal } from "@xterm/xterm";

export interface TerminalScroll {
  pages: number;
}

/** Turns line-sized scroll movement into local scrolling or bounded page requests. */
export interface ViewerPager {
  move(lines: number): void;
  /** Drop the sub-page remainder and the gesture's page count, e.g. when a new gesture starts. */
  reset(): void;
}

/** Height of one terminal row in CSS pixels, from the rendered screen. */
export function terminalLineHeight(terminal: Terminal, host: HTMLElement): number {
  const screen = host.querySelector(".xterm-screen") ?? host;
  return screen.getBoundingClientRect().height / terminal.rows || 18;
}

/** Pause between wheel events (ms) after which the next event starts a new gesture. */
export const GESTURE_GAP_MS = 250;
/** Most pages one wheel gesture may send. */
export const MAX_PAGES_PER_GESTURE = 3;

export interface ViewerPagerOptions {
  /**
   * Gesture paging for the alternate screen: the first page needs a short lead
   * (a quarter viewport, at least three lines), later pages a full viewport, up
   * to MAX_PAGES_PER_GESTURE per gesture. Off keeps one viewport per page.
   */
  gesture?: boolean;
}

export function createViewerPager(
  terminal: Terminal,
  scroll: (event: TerminalScroll) => void,
  { gesture = false }: ViewerPagerOptions = {},
): ViewerPager {
  let remainder = 0;
  let sent = 0;
  return {
    reset() {
      remainder = 0;
      sent = 0;
    },
    move(delta) {
      if (Math.sign(delta) !== Math.sign(remainder)) remainder = 0;
      remainder += delta;
      if (terminal.buffer.active.type === "normal") {
        const lines = Math.trunc(remainder);
        remainder -= lines;
        if (lines !== 0) terminal.scrollLines(lines);
        return;
      }
      // In tmux's alternate screen, xterm otherwise synthesizes Up/Down,
      // which recalls prompts. Use the application's PageUp/PageDown instead.
      // Touch (no gesture) pages match the visible viewport, so one notch never
      // amplifies into a whole page. Gesture mode leads with a quarter viewport
      // (max(3, ceil(rows / 4)) lines), then full viewports, capped per gesture.
      const pageSize = terminal.rows;
      if (!gesture) {
        const pages = Math.max(-1, Math.min(1, Math.trunc(remainder / pageSize)));
        remainder %= pageSize;
        if (pages !== 0) scroll({ pages });
        return;
      }
      const needed = sent === 0 ? Math.max(3, Math.ceil(pageSize / 4)) : pageSize;
      if (sent >= MAX_PAGES_PER_GESTURE || Math.abs(remainder) < needed) return;
      const pages = Math.sign(remainder);
      remainder -= pages * needed;
      sent += 1;
      scroll({ pages });
    },
  };
}

/** Forward only wheel gestures while stdin remains disabled. */
export function installViewerWheel(
  terminal: Terminal,
  host: HTMLElement,
  scroll: (event: TerminalScroll) => void,
): () => void {
  const pager = createViewerPager(terminal, scroll, { gesture: true });
  let lastTime = Number.NEGATIVE_INFINITY;
  const onWheel = (event: WheelEvent) => {
    if (!terminal.options.disableStdin || event.ctrlKey) return;
    // On the normal buffer a zero-delta event has nothing to scroll and xterm
    // synthesizes no keys there, so leave it alone (the read-only history
    // emulator installs this handler on the normal buffer).
    if (terminal.buffer.active.type === "normal" && event.deltaY === 0) return;
    // On the alternate screen cancel even a zero-delta event so a sideways
    // trackpad swipe cannot reach xterm, which would synthesize arrow keys.
    event.preventDefault();
    event.stopPropagation();
    if (event.deltaY === 0) return;
    // Only the alternate screen starts a new page gesture after a pause. Normal-buffer
    // scrolling keeps its sub-line remainder, so slow pixel wheels still move lines.
    if (terminal.buffer.active.type === "alternate" && event.timeStamp - lastTime > GESTURE_GAP_MS)
      pager.reset();
    lastTime = event.timeStamp;
    pager.move(
      event.deltaMode === WheelEvent.DOM_DELTA_PAGE
        ? event.deltaY * terminal.rows
        : event.deltaMode === WheelEvent.DOM_DELTA_LINE
          ? event.deltaY
          : event.deltaY / terminalLineHeight(terminal, host),
    );
  };
  host.addEventListener("wheel", onWheel, { capture: true, passive: false });
  return () => host.removeEventListener("wheel", onWheel, true);
}
