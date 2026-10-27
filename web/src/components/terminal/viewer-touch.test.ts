import { Terminal } from "@xterm/xterm";
import { afterEach, describe, expect, it, vi } from "vitest";
import { installViewerTouch } from "./viewer-touch";

const disposers: (() => void)[] = [];
afterEach(() =>
  disposers
    .splice(0)
    .reverse()
    .forEach((dispose) => dispose()),
);

// jsdom lays nothing out, so a row falls back to 18px: rows 4 = a 72px page.
function viewer(readOnly = true) {
  const terminal = new Terminal({ allowProposedApi: true, rows: 4, disableStdin: readOnly });
  const host = document.createElement("div");
  const child = host.appendChild(document.createElement("div"));
  const scroll = vi.fn();
  disposers.push(() => terminal.dispose(), installViewerTouch(terminal, host, scroll));
  const write = (data: string) => new Promise<void>((resolve) => terminal.write(data, resolve));
  const touch = (type: string, ...ys: number[]) => {
    const event = new Event(type, { bubbles: true, cancelable: true });
    Object.defineProperty(event, "touches", { value: ys.map((clientY) => ({ clientY })) });
    child.dispatchEvent(event);
    return event;
  };
  const swipe = (from: number, ...steps: number[]) => {
    touch("touchstart", from);
    const moves = steps.map((y) => touch("touchmove", y));
    touch("touchend");
    return moves;
  };
  return { terminal, child, scroll, write, touch, swipe };
}

describe("viewer touch", () => {
  it("pages the alternate screen without sending keystrokes", async () => {
    const { terminal, child, scroll, write, swipe } = viewer();
    await write("\x1b[?1049h");
    const input = vi.fn();
    const xtermTouch = vi.fn();
    terminal.onData(input);
    child.addEventListener("touchmove", xtermTouch);
    const moves = swipe(300, 264, 228);
    expect(moves.every((event) => event.defaultPrevented)).toBe(true);
    expect(scroll.mock.calls).toEqual([[{ pages: 1 }]]);
    swipe(100, 172);
    expect(scroll.mock.calls).toEqual([[{ pages: 1 }], [{ pages: -1 }]]);
    expect(xtermTouch).not.toHaveBeenCalled();
    expect(input).not.toHaveBeenCalled();
  });

  it("accumulates moves within a swipe to a page and bounds one large move to one page", async () => {
    const { scroll, write, swipe } = viewer();
    await write("\x1b[?1049h");
    swipe(300, 250);
    expect(scroll).not.toHaveBeenCalled();
    swipe(300, 270, 250, 228);
    expect(scroll.mock.calls).toEqual([[{ pages: 1 }]]);
    swipe(600, 0);
    expect(scroll.mock.calls).toEqual([[{ pages: 1 }], [{ pages: 1 }]]);
  });

  it("starts each swipe from zero instead of carrying the last swipe's remainder", async () => {
    const { scroll, write, swipe } = viewer();
    await write("\x1b[?1049h");
    swipe(300, 240);
    swipe(300, 280);
    expect(scroll).not.toHaveBeenCalled();
  });

  it("leaves two-finger gestures to the browser", async () => {
    const { scroll, write, touch } = viewer();
    await write("\x1b[?1049h");
    touch("touchstart", 300, 400);
    expect(touch("touchmove", 200, 500).defaultPrevented).toBe(false);
    expect(scroll).not.toHaveBeenCalled();
  });

  it("leaves the normal buffer to xterm's touch scrolling", async () => {
    const { scroll, write, swipe } = viewer();
    await write("0\r\n1\r\n2\r\n3\r\n4\r\n5");
    expect(swipe(300, 200)[0].defaultPrevented).toBe(false);
    expect(scroll).not.toHaveBeenCalled();
  });

  it("leaves control-mode touch handling unchanged", async () => {
    const { scroll, write, swipe } = viewer(false);
    await write("\x1b[?1049h");
    expect(swipe(300, 200)[0].defaultPrevented).toBe(false);
    expect(scroll).not.toHaveBeenCalled();
  });
});
