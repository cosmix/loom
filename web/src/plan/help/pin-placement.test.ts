import { describe, expect, it } from "vitest";

import { anchorBox, CANVAS_MARGIN, PIN_GAP, PIN_SIZE, placePins, type Box } from "./pin-placement";

const bounds = { width: 300, height: 200 };

function overlaps(a: Box, b: Box): boolean {
  return a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y;
}

function expectInside(pin: Box): void {
  expect(pin.x).toBeGreaterThanOrEqual(0);
  expect(pin.y).toBeGreaterThanOrEqual(0);
  expect(pin.x + pin.w).toBeLessThanOrEqual(bounds.width);
  expect(pin.y + pin.h).toBeLessThanOrEqual(bounds.height);
}

describe("placePins side selection", () => {
  it("uses the first free side above the target", () => {
    expect(placePins([{ x: 100, y: 80, w: 40, h: 20 }], bounds)).toEqual([
      { x: 109, y: 52, w: 22, h: 22 },
    ]);
    expect([PIN_SIZE, PIN_GAP, CANVAS_MARGIN]).toEqual([22, 6, 32]);
  });

  it("moves left when a later target blocks above", () => {
    const targets = [
      { x: 100, y: 80, w: 40, h: 20 },
      { x: 109, y: 52, w: 22, h: 22 },
    ];
    expect(placePins(targets, bounds)[0]).toEqual({ x: 72, y: 79, w: 22, h: 22 });
  });

  it("keeps adjacent targets pins apart including their clearance", () => {
    const targets = [
      { x: 100, y: 80, w: 20, h: 20 },
      { x: 123, y: 80, w: 20, h: 20 },
    ];
    expect(placePins(targets, bounds)).toEqual([
      { x: 99, y: 52, w: 22, h: 22 },
      { x: 122, y: 106, w: 22, h: 22 },
    ]);
  });

  it("uses left at the top edge and below at the top left corner", () => {
    expect(placePins([{ x: 100, y: 0, w: 40, h: 40 }], bounds)).toEqual([
      { x: 72, y: 9, w: 22, h: 22 },
    ]);
    expect(placePins([{ x: 0, y: 0, w: 40, h: 20 }], bounds)).toEqual([
      { x: 9, y: 26, w: 22, h: 22 },
    ]);
  });
});

describe("placePins inputs", () => {
  it("preserves null targets and empty input", () => {
    expect(placePins([null, { x: 100, y: 80, w: 40, h: 20 }, null], bounds)).toEqual([
      null,
      { x: 109, y: 52, w: 22, h: 22 },
      null,
    ]);
    expect(placePins([], bounds)).toEqual([]);
  });

  it("supports a custom pin size", () => {
    expect(placePins([{ x: 100, y: 80, w: 40, h: 20 }], bounds, 10)).toEqual([
      { x: 115, y: 64, w: 10, h: 10 },
    ]);
  });
});

describe("placePins fallback", () => {
  it("chooses the least overlap among blocked fallback sides", () => {
    const targets = [
      { x: 100, y: 80, w: 40, h: 20 },
      { x: 109, y: 52, w: 22, h: 22 },
      { x: 72, y: 79, w: 22, h: 22 },
      { x: 109, y: 106, w: 22, h: 22 },
      { x: 146, y: 79, w: 1, h: 22 },
    ];
    expect(placePins(targets, bounds)[0]).toEqual({ x: 146, y: 79, w: 22, h: 22 });
  });

  it("scores fallback overlap after clamping into bounds", () => {
    const targets = [
      { x: 0, y: 50, w: 10, h: 20 },
      { x: 0, y: 22, w: 22, h: 22 },
      { x: 0, y: 76, w: 1, h: 22 },
      { x: 16, y: 49, w: 22, h: 22 },
    ];
    expect(placePins(targets, bounds)[0]).toEqual({ x: 0, y: 76, w: 22, h: 22 });
  });

  it("avoids its own target even when another fallback side has less overlap", () => {
    const targets = [
      { x: 0, y: 0, w: 40, h: 20 },
      { x: 9, y: 26, w: 22, h: 22 },
      { x: 46, y: 0, w: 22, h: 22 },
    ];
    expect(placePins(targets, bounds)[0]).toEqual({ x: 9, y: 26, w: 22, h: 22 });
  });
});

describe("placePins occupied fallback", () => {
  it("includes placed pins in fallback overlap scores", () => {
    const targets = [
      { x: 100, y: 80, w: 40, h: 20 },
      { x: 100, y: 80, w: 40, h: 20 },
      { x: 72, y: 79, w: 1, h: 22 },
      { x: 109, y: 106, w: 1, h: 22 },
      { x: 146, y: 79, w: 1, h: 22 },
    ];
    expect(placePins(targets, bounds).slice(0, 2)).toEqual([
      { x: 109, y: 52, w: 22, h: 22 },
      { x: 72, y: 79, w: 22, h: 22 },
    ]);
  });
});

describe("placePins invariants", () => {
  it("keeps every pin inside bounds and outside its own target", () => {
    const targets = [
      { x: 0, y: 0, w: 12, h: 12 },
      { x: 288, y: 0, w: 12, h: 12 },
      { x: 0, y: 188, w: 12, h: 12 },
      { x: 288, y: 188, w: 12, h: 12 },
      { x: 100, y: 80, w: 40, h: 20 },
    ];
    const pins = placePins(targets, bounds);
    pins.forEach((pin, index) => {
      const target = targets[index];
      expect(pin).not.toBeNull();
      if (pin === null || target === undefined) throw new Error("Missing pin or target");
      expectInside(pin);
      expect(overlaps(pin, target)).toBe(false);
      for (const other of targets) expect(overlaps(pin, other)).toBe(false);
      for (const previous of pins.slice(0, index)) {
        if (previous !== null) expect(overlaps(pin, previous)).toBe(false);
      }
    });
  });
});

describe("anchorBox", () => {
  it("returns the union of the whole control group", () => {
    expect(
      anchorBox([
        { x: 20, y: 30, w: 40, h: 10 },
        { x: -5, y: 50, w: 20, h: 30 },
        { x: 10, y: 5, w: 80, h: 15 },
      ]),
    ).toEqual({ x: -5, y: 5, w: 95, h: 75 });
  });

  it("returns null for an empty group and preserves a single box", () => {
    expect(anchorBox([])).toBeNull();
    expect(anchorBox([{ x: 10, y: 20, w: 30, h: 40 }])).toEqual({
      x: 10,
      y: 20,
      w: 30,
      h: 40,
    });
  });
});
