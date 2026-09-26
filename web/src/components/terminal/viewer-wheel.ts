import type { Terminal } from "@xterm/xterm";

export interface TerminalScroll {
  pages: number;
}

/** Forward only wheel gestures while stdin remains disabled. */
export function installViewerWheel(
  terminal: Terminal,
  host: HTMLElement,
  scroll: (event: TerminalScroll) => void,
): () => void {
  let remainder = 0;
  const onWheel = (event: WheelEvent) => {
    if (!terminal.options.disableStdin || event.ctrlKey || event.deltaY === 0) return;
    event.preventDefault();
    event.stopPropagation();
    const screen = host.querySelector(".xterm-screen") ?? host;
    const rect = screen.getBoundingClientRect();
    const lineHeight = rect.height / terminal.rows || 18;
    const delta =
      event.deltaMode === WheelEvent.DOM_DELTA_PAGE
        ? event.deltaY * terminal.rows
        : event.deltaMode === WheelEvent.DOM_DELTA_LINE
          ? event.deltaY
          : event.deltaY / lineHeight;
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
  };
  host.addEventListener("wheel", onWheel, { capture: true, passive: false });
  return () => host.removeEventListener("wheel", onWheel, true);
}
