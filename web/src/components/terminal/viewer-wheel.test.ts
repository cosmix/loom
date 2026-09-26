import { Terminal } from "@xterm/xterm";
import { afterEach, describe, expect, it, vi } from "vitest";
import { installViewerWheel } from "./viewer-wheel";

const disposers: (() => void)[] = [];
afterEach(() =>
  disposers
    .splice(0)
    .reverse()
    .forEach((dispose) => dispose()),
);

function viewer(readOnly = true) {
  const terminal = new Terminal({ allowProposedApi: true, rows: 4, disableStdin: readOnly });
  const host = document.createElement("div");
  const child = host.appendChild(document.createElement("div"));
  const scroll = vi.fn();
  disposers.push(() => terminal.dispose(), installViewerWheel(terminal, host, scroll));
  const write = (data: string) => new Promise<void>((resolve) => terminal.write(data, resolve));
  const wheel = (deltaY: number, deltaMode: number = WheelEvent.DOM_DELTA_LINE) => {
    const event = new WheelEvent("wheel", { deltaY, deltaMode, bubbles: true, cancelable: true });
    child.dispatchEvent(event);
    return event;
  };
  return { terminal, child, scroll, write, wheel };
}

describe("viewer wheel", () => {
  it("pages backward and forward instead of synthesizing prompt-recall arrows", async () => {
    const { terminal, child, scroll, write, wheel } = viewer();
    await write("\x1b[?1049h");
    const input = vi.fn();
    const xtermWheel = vi.fn();
    terminal.onData(input);
    child.addEventListener("wheel", xtermWheel);
    expect(wheel(-4).defaultPrevented).toBe(true);
    wheel(4);
    expect(scroll.mock.calls).toEqual([[{ pages: -1 }], [{ pages: 1 }]]);
    expect(xtermWheel).not.toHaveBeenCalled();
    expect(input).not.toHaveBeenCalled();
    expect(terminal.options.disableStdin).toBe(true);
  });

  it("accumulates small trackpad deltas and bounds large gestures", async () => {
    const { scroll, write, wheel } = viewer();
    await write("\x1b[?1049h");
    wheel(-27, WheelEvent.DOM_DELTA_PIXEL);
    expect(scroll).not.toHaveBeenCalled();
    wheel(-27, WheelEvent.DOM_DELTA_PIXEL);
    expect(scroll).not.toHaveBeenCalled();
    wheel(-18, WheelEvent.DOM_DELTA_PIXEL);
    expect(scroll).toHaveBeenLastCalledWith({ pages: -1 });
    wheel(1, WheelEvent.DOM_DELTA_PAGE);
    expect(scroll).toHaveBeenLastCalledWith({ pages: 1 });
    wheel(300);
    expect(scroll).toHaveBeenLastCalledWith({ pages: 1 });
    wheel(-300);
    expect(scroll).toHaveBeenLastCalledWith({ pages: -1 });
  });

  it("requires a viewport of wheel movement and resets on reversal", async () => {
    const { terminal, scroll, write, wheel } = viewer();
    terminal.resize(80, 24);
    await write("\x1b[?1049h");
    wheel(-3);
    wheel(-20);
    expect(scroll).not.toHaveBeenCalled();
    wheel(-1);
    expect(scroll.mock.calls).toEqual([[{ pages: -1 }]]);
    wheel(-23);
    wheel(3);
    expect(scroll).toHaveBeenCalledOnce();
    wheel(21);
    expect(scroll.mock.calls).toEqual([[{ pages: -1 }], [{ pages: 1 }]]);
  });

  it("scrolls the normal buffer locally", async () => {
    const { terminal, scroll, write, wheel } = viewer();
    await write("0\r\n1\r\n2\r\n3\r\n4\r\n5");
    const bottom = terminal.buffer.active.baseY;
    wheel(-1);
    expect(terminal.buffer.active.viewportY).toBe(bottom - 1);
    wheel(1);
    expect(terminal.buffer.active.viewportY).toBe(bottom);
    expect(scroll).not.toHaveBeenCalled();
  });

  it("leaves control-mode wheel handling unchanged", async () => {
    const { child, scroll, write, wheel } = viewer(false);
    await write("\x1b[?1049h");
    const xtermWheel = vi.fn();
    child.addEventListener("wheel", xtermWheel);
    expect(wheel(-3).defaultPrevented).toBe(false);
    expect(xtermWheel).toHaveBeenCalledOnce();
    expect(scroll).not.toHaveBeenCalled();
  });
});
