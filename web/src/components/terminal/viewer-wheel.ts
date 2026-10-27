import type { Terminal } from "@xterm/xterm";

export interface TerminalScroll {
  pages: number;
}

/** Turns line-sized scroll movement into local scrolling or bounded page requests. */
export interface ViewerPager {
  move(lines: number): void;
  /** Drop the sub-page remainder, e.g. when a new gesture starts. */
  reset(): void;
}

/** Height of one terminal row in CSS pixels, from the rendered screen. */
export function terminalLineHeight(terminal: Terminal, host: HTMLElement): number {
  const screen = host.querySelector(".xterm-screen") ?? host;
  return screen.getBoundingClientRect().height / terminal.rows || 18;
}

export function createViewerPager(
  terminal: Terminal,
  scroll: (event: TerminalScroll) => void,
): ViewerPager {
  let remainder = 0;
  return {
    reset() {
      remainder = 0;
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
      // Match a page to the visible viewport, rather than amplifying a wheel
      // notch into a whole page. Large gestures must not skip several pages.
      const pageSize = terminal.rows;
      const pages = Math.max(-1, Math.min(1, Math.trunc(remainder / pageSize)));
      remainder %= pageSize;
      if (pages !== 0) scroll({ pages });
    },
  };
}

/** Forward only wheel gestures while stdin remains disabled. */
export function installViewerWheel(
  terminal: Terminal,
  host: HTMLElement,
  scroll: (event: TerminalScroll) => void,
): () => void {
  const pager = createViewerPager(terminal, scroll);
  const onWheel = (event: WheelEvent) => {
    if (!terminal.options.disableStdin || event.ctrlKey || event.deltaY === 0) return;
    event.preventDefault();
    event.stopPropagation();
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
