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
  let clock = 10_000;
  // Events 16 ms apart form one gesture unless a test sets `at` explicitly.
  const wheel = (
    deltaY: number,
    deltaMode: number = WheelEvent.DOM_DELTA_LINE,
    at: number = (clock += 16),
    wheelDeltaY?: number,
  ) => {
    const event = new WheelEvent("wheel", { deltaY, deltaMode, bubbles: true, cancelable: true });
    Object.defineProperty(event, "timeStamp", { value: at });
    if (wheelDeltaY !== undefined)
      Object.defineProperty(event, "wheelDeltaY", { value: wheelDeltaY });
    clock = at;
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
    // Four rows: the lead page needs three lines (54 px at the 18 px fallback).
    wheel(-27, WheelEvent.DOM_DELTA_PIXEL);
    expect(scroll).not.toHaveBeenCalled();
    wheel(-27, WheelEvent.DOM_DELTA_PIXEL);
    expect(scroll).toHaveBeenLastCalledWith({ pages: -1 });
    // A page-mode delta moves a whole viewport and reverses the gesture.
    wheel(1, WheelEvent.DOM_DELTA_PAGE);
    expect(scroll).toHaveBeenLastCalledWith({ pages: 1 });
    wheel(300);
    expect(scroll).toHaveBeenLastCalledWith({ pages: 1 });
    expect(scroll).toHaveBeenCalledTimes(3);
    wheel(-300);
    expect(scroll).toHaveBeenCalledTimes(3);
  });

  it("needs a lead distance, then a viewport per page, and resets on reversal", async () => {
    const { terminal, scroll, write, wheel } = viewer();
    terminal.resize(80, 24);
    await write("\x1b[?1049h");
    // Lead page: max(3, ceil(24 / 4)) = 6 lines.
    wheel(-3);
    wheel(-2);
    expect(scroll).not.toHaveBeenCalled();
    wheel(-1);
    expect(scroll.mock.calls).toEqual([[{ pages: -1 }]]);
    // Later pages need a full 24 lines.
    wheel(-23);
    expect(scroll).toHaveBeenCalledOnce();
    wheel(3);
    wheel(20);
    expect(scroll).toHaveBeenCalledOnce();
    wheel(1);
    expect(scroll.mock.calls).toEqual([[{ pages: -1 }], [{ pages: 1 }]]);
  });

  it("scrolls trackpad input in proportion to the movement", async () => {
    const { terminal, scroll, write, wheel } = viewer();
    terminal.resize(80, 24);
    await write("\x1b[?1049h");
    const touch = (lines: number, at?: number) =>
      wheel(lines, WheelEvent.DOM_DELTA_LINE, at, -lines * 3);
    // Lead is 6 lines per page. Opposite jitter nets out instead of dropping the lead.
    touch(2);
    touch(-1);
    touch(3);
    touch(2);
    expect(scroll.mock.calls).toEqual([[{ pages: 1 }]]);
    // Fast events accelerate: 8 lines (x3) is four pages, where 1:1 would be one.
    // The long pause starts a new gesture, so the remainder is zero.
    touch(8, 20_000);
    expect(scroll).toHaveBeenLastCalledWith({ pages: 4 });
    // A huge delta is bounded to one frame and leaves no late scroll behind.
    touch(500);
    expect(scroll).toHaveBeenLastCalledWith({ pages: 5 });
    const calls = scroll.mock.calls.length;
    touch(1);
    expect(scroll).toHaveBeenCalledTimes(calls);
    // Continuous momentum events keep paging with no pause and no time budget.
    for (let t = 20_100; t < 21_200; t += 100) touch(6, t);
    expect(scroll.mock.calls.length).toBeGreaterThan(calls + 8);
  });

  it("leaves classic wheel ticks on the original rules", async () => {
    const { terminal, scroll, write, wheel } = viewer();
    terminal.resize(80, 24);
    await write("\x1b[?1049h");
    const tick = (lines: number, at?: number) =>
      wheel(lines, WheelEvent.DOM_DELTA_LINE, at, -Math.sign(lines) * 120);
    tick(-6, 20_000);
    expect(scroll).toHaveBeenCalledTimes(1);
    // Events 200 ms apart are one gesture: the budget does not renew by time.
    for (let i = 1; i <= 6; i += 1) tick(-24, 20_000 + i * 200);
    expect(scroll).toHaveBeenCalledTimes(3);
    // Reversal drops the remainder rather than netting it.
    tick(-20, 20_000 + 1_400);
    tick(30, 20_000 + 1_500);
    expect(scroll).toHaveBeenCalledTimes(3);
  });

  it("prevents a zero-delta wheel in View mode without paging", async () => {
    const { child, scroll, write, wheel } = viewer();
    await write("\x1b[?1049h");
    const xtermWheel = vi.fn();
    child.addEventListener("wheel", xtermWheel);
    expect(wheel(0).defaultPrevented).toBe(true);
    expect(xtermWheel).not.toHaveBeenCalled();
    expect(scroll).not.toHaveBeenCalled();
  });

  it("does not cancel a zero-delta wheel on the normal buffer", async () => {
    const { child, scroll, write, wheel } = viewer();
    await write("0\r\n1\r\n2");
    const xtermWheel = vi.fn();
    child.addEventListener("wheel", xtermWheel);
    expect(wheel(0).defaultPrevented).toBe(false);
    expect(xtermWheel).toHaveBeenCalledOnce();
    expect(scroll).not.toHaveBeenCalled();
  });

  it("starts a new gesture after a pause", async () => {
    const { scroll, write, wheel } = viewer();
    await write("\x1b[?1049h");
    // Four rows: the lead page needs three lines.
    wheel(1, WheelEvent.DOM_DELTA_LINE, 1_000);
    wheel(1, WheelEvent.DOM_DELTA_LINE, 1_100);
    expect(scroll).not.toHaveBeenCalled();
    // The 400 ms pause drops the first two lines; without it this would reach three.
    wheel(1, WheelEvent.DOM_DELTA_LINE, 1_500);
    expect(scroll).not.toHaveBeenCalled();
    wheel(2, WheelEvent.DOM_DELTA_LINE, 1_600);
    expect(scroll.mock.calls).toEqual([[{ pages: 1 }]]);
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

  it("keeps the normal-buffer remainder across a pause", async () => {
    const { terminal, scroll, write, wheel } = viewer();
    await write("0\r\n1\r\n2\r\n3\r\n4\r\n5");
    const bottom = terminal.buffer.active.baseY;
    // 10 px at the 18 px fallback is 0.56 lines: the first event moves nothing.
    wheel(-10, WheelEvent.DOM_DELTA_PIXEL, 1_000);
    expect(terminal.buffer.active.viewportY).toBe(bottom);
    // 300 ms later the remainder (1.11 lines) still counts, so one line scrolls.
    wheel(-10, WheelEvent.DOM_DELTA_PIXEL, 1_300);
    expect(terminal.buffer.active.viewportY).toBe(bottom - 1);
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
