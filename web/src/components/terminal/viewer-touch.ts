import type { Terminal } from "@xterm/xterm";
import { createViewerPager, terminalLineHeight, type TerminalScroll } from "./viewer-wheel";

/**
 * Page a viewed alternate screen with a one-finger vertical swipe.
 *
 * xterm's own touch scrolling moves only its scrollback viewport, which is
 * empty while tmux holds the alternate screen, so a swipe would do nothing.
 * The swipe feeds the same bounded page requests as the wheel: a finger
 * moving up by one terminal height asks for one page of newer output. The
 * normal buffer and Control mode keep xterm's native touch handling.
 */
export function installViewerTouch(
  terminal: Terminal,
  host: HTMLElement,
  scroll: (event: TerminalScroll) => void,
): () => void {
  const pager = createViewerPager(terminal, scroll);
  let lastY: number | null = null;

  const singleY = (event: TouchEvent) =>
    event.touches.length === 1 ? event.touches[0].clientY : null;
  const onMove = (event: TouchEvent) => {
    const y = singleY(event);
    const paging = terminal.options.disableStdin && terminal.buffer.active.type === "alternate";
    if (y === null || lastY === null || !paging) {
      lastY = y;
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    pager.move((lastY - y) / terminalLineHeight(terminal, host));
    lastY = y;
  };
  const onTouch = (event: TouchEvent) => {
    lastY = singleY(event);
  };
  // Each swipe pages on its own distance: a leftover from the last one would
  // make a short swipe page early.
  const onStart = (event: TouchEvent) => {
    pager.reset();
    onTouch(event);
  };

  host.addEventListener("touchstart", onStart, { capture: true, passive: true });
  host.addEventListener("touchmove", onMove, { capture: true, passive: false });
  host.addEventListener("touchend", onTouch, { capture: true, passive: true });
  host.addEventListener("touchcancel", onTouch, { capture: true, passive: true });
  return () => {
    host.removeEventListener("touchstart", onStart, true);
    host.removeEventListener("touchmove", onMove, true);
    host.removeEventListener("touchend", onTouch, true);
    host.removeEventListener("touchcancel", onTouch, true);
  };
}
