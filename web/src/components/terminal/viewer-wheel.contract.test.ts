import { Terminal } from "@xterm/xterm";
import { afterEach, describe, expect, it, vi } from "vitest";

import { installViewerWheel } from "./viewer-wheel";

// A 45-row View-mode viewer on tmux's alternate screen. jsdom renders no screen,
// so a pixel delta converts to lines at the 18 px fallback row height: the lead
// page needs max(3, ceil(45 / 4)) = 12 lines, each later page a full 45.
const ROWS = 45;
const PIXEL = WheelEvent.DOM_DELTA_PIXEL;
const LINE = WheelEvent.DOM_DELTA_LINE;

const disposers: (() => void)[] = [];
afterEach(() =>
  disposers
    .splice(0)
    .reverse()
    .forEach((dispose) => dispose()),
);

interface Wheel {
  mode?: number;
  /// Milliseconds since the previous event of this viewer.
  gap?: number;
  deltaX?: number;
  ctrlKey?: boolean;
}

async function viewer(readOnly = true) {
  const terminal = new Terminal({ allowProposedApi: true, rows: ROWS, disableStdin: readOnly });
  const host = document.createElement("div");
  const child = host.appendChild(document.createElement("div"));
  const scroll = vi.fn();
  disposers.push(() => terminal.dispose(), installViewerWheel(terminal, host, scroll));
  await new Promise<void>((resolve) => terminal.write("\x1b[?1049h", resolve));
  let clock = 10_000;
  const wheel = (
    deltaY: number,
    { mode = PIXEL, gap = 16, deltaX = 0, ctrlKey = false }: Wheel = {},
  ) => {
    clock += gap;
    const event = new WheelEvent("wheel", {
      deltaX,
      deltaY,
      deltaMode: mode,
      ctrlKey,
      bubbles: true,
      cancelable: true,
    });
    Object.defineProperty(event, "timeStamp", { value: clock });
    child.dispatchEvent(event);
    return event;
  };
  return { child, scroll, wheel, sends: () => scroll.mock.calls.length };
}

describe("viewer wheel gestures", () => {
  it("a gentle trackpad swipe sends one page", async () => {
    const { scroll, wheel } = await viewer();
    for (let event = 0; event < 20; event += 1) wheel(15);
    expect(scroll.mock.calls).toEqual([[{ pages: 1 }]]);
  });

  it("a single slow mouse notch sends no page", async () => {
    const notch = await viewer();
    notch.wheel(40);
    expect(notch.scroll).not.toHaveBeenCalled();

    const joined = await viewer();
    joined.wheel(120);
    joined.wheel(120, { gap: 250 });
    expect(joined.scroll.mock.calls).toEqual([[{ pages: 1 }]]);

    const split = await viewer();
    split.wheel(120);
    split.wheel(120, { gap: 251 });
    expect(split.scroll).not.toHaveBeenCalled();
  });

  it("a hard flick sends at most three pages one per event", async () => {
    const { scroll, wheel, sends } = await viewer();
    wheel(3 * ROWS + 5, { mode: LINE });
    expect(sends()).toBe(1);
    wheel(ROWS, { mode: LINE });
    expect(sends()).toBe(2);
    wheel(ROWS, { mode: LINE });
    expect(sends()).toBe(3);
    wheel(3 * ROWS, { mode: LINE });
    wheel(ROWS, { mode: LINE });
    expect(sends()).toBe(3);
    wheel(-3 * ROWS, { mode: LINE });
    wheel(-ROWS, { mode: LINE });
    expect(sends()).toBe(3);
    wheel(-12, { mode: LINE, gap: 300 });
    expect(scroll.mock.calls).toEqual([
      [{ pages: 1 }],
      [{ pages: 1 }],
      [{ pages: 1 }],
      [{ pages: -1 }],
    ]);

    const fresh = await viewer();
    fresh.wheel(11, { mode: LINE });
    expect(fresh.sends()).toBe(0);
    fresh.wheel(1, { mode: LINE });
    expect(fresh.sends()).toBe(1);
    fresh.wheel(ROWS - 1, { mode: LINE });
    expect(fresh.sends()).toBe(1);
    fresh.wheel(1, { mode: LINE });
    expect(fresh.scroll.mock.calls).toEqual([[{ pages: 1 }], [{ pages: 1 }]]);
  });

  it("a wheel with no vertical movement stays in the viewer", async () => {
    const view = await viewer();
    const xterm = vi.fn();
    view.child.addEventListener("wheel", xterm);
    const sideways = view.wheel(0, { deltaX: 30 });
    expect(sideways.defaultPrevented).toBe(true);
    expect(xterm).not.toHaveBeenCalled();
    expect(view.scroll).not.toHaveBeenCalled();

    const pinch = view.wheel(0, { deltaX: 30, ctrlKey: true });
    expect(pinch.defaultPrevented).toBe(false);
    expect(xterm).toHaveBeenCalledOnce();

    const control = await viewer(false);
    const controlXterm = vi.fn();
    control.child.addEventListener("wheel", controlXterm);
    const live = control.wheel(0, { deltaX: 30 });
    expect(live.defaultPrevented).toBe(false);
    expect(controlXterm).toHaveBeenCalledOnce();
    expect(control.scroll).not.toHaveBeenCalled();
  });
});
