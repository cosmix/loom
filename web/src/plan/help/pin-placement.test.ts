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

describe("placePins bars and stacked targets", () => {
  // A bar is a wide, short target under a busy row; its pin hung off the far left end of it. A bar
  // takes the side below it before left or right. The pins of the Getting around topic
  // (help/topics.ts) are placed by the same rule.
  it("centres a bar's pin below it instead of at its far left end", () => {
    const tall = { width: 300, height: 260 };
    const bar = { x: 20, y: 150, w: 260, h: 30 };
    const row = { x: 100, y: 100, w: 100, h: 44 };
    const [pin] = placePins([bar, row], tall);

    expect(pin).toEqual({ x: 139, y: 186, w: 22, h: 22 });
    expect(pin!.x + pin!.w / 2).toBe(bar.x + bar.w / 2);
    expect(overlaps(pin!, bar)).toBe(false);
  });

  it("keeps the side order for a target that is not a bar", () => {
    const control = { x: 100, y: 80, w: 40, h: 20 };
    const blocker = { x: 109, y: 52, w: 22, h: 22 };

    expect(placePins([control, blocker], bounds)[0]).toEqual({ x: 72, y: 79, w: 22, h: 22 });
  });

  // Pressure, pin 3: cards that share their edges leave no free side; the pin used to float at a
  // clamped spot away from its card.
  it("places a pin inside the corner of a card whose every side is taken", () => {
    const cards = [
      { x: 0, y: 0, w: 200, h: 60 },
      { x: 0, y: 60, w: 200, h: 60 },
      { x: 0, y: 120, w: 200, h: 80 },
    ];
    const pins = placePins(cards, { width: 200, height: 200 });

    expect(pins[2]).toEqual({ x: 6, y: 126, w: 22, h: 22 });
    pins.forEach((pin, index) => {
      const card = cards[index]!;
      expect(pin!.x).toBeGreaterThanOrEqual(card.x);
      expect(pin!.y).toBeGreaterThanOrEqual(card.y);
      expect(pin!.x + pin!.w).toBeLessThanOrEqual(card.x + card.w);
      expect(pin!.y + pin!.h).toBeLessThanOrEqual(card.y + card.h);
    });
  });

  it("does not sit inside a target too small to hold a pin", () => {
    const [pin] = placePins([{ x: 100, y: 80, w: 20, h: 20 }], bounds);

    expect(overlaps(pin!, { x: 100, y: 80, w: 20, h: 20 })).toBe(false);
  });
});

// Chat scene, pin 1: the chip's remove button sat under the pin; pressure, pin 3: it sat on a line
// of text. Text runs and controls the stage holds are boxes a side may not cover.
describe("placePins occupied boxes", () => {
  const target = { x: 100, y: 80, w: 40, h: 20 };

  it("skips a side that would cover a text run or a control", () => {
    const text = { x: 90, y: 50, w: 70, h: 16 };

    expect(placePins([target], bounds, PIN_SIZE, { occupied: [text] })[0]).toEqual({
      x: 72,
      y: 79,
      w: 22,
      h: 22,
    });
    const control = { x: 60, y: 70, w: 36, h: 36 };
    expect(placePins([target], bounds, PIN_SIZE, { occupied: [text, control] })[0]).toEqual({
      x: 109,
      y: 106,
      w: 22,
      h: 22,
    });
  });

  it("keeps the side order when nothing is occupied", () => {
    expect(placePins([target], bounds, PIN_SIZE, { occupied: [] })[0]).toEqual({
      x: 109,
      y: 52,
      w: 22,
      h: 22,
    });
  });
});

// A stage inset 60px from each canvas edge, as a panel scene's is, leaves a gutter either side.
describe("placePins gutter", () => {
  const stage = { x: 60, y: 0, w: 180, h: 200 };
  const target = { x: 100, y: 80, w: 40, h: 20 };
  const surrounded = [
    { x: 90, y: 50, w: 70, h: 16 },
    { x: 60, y: 70, w: 36, h: 36 },
    { x: 100, y: 106, w: 40, h: 16 },
    { x: 146, y: 70, w: 30, h: 36 },
  ];

  it("uses the margin level with the target's centre when no side is clean", () => {
    const [pin] = placePins([target], bounds, PIN_SIZE, { occupied: surrounded, stage });

    expect(pin).toEqual({ x: 32, y: 79, w: 22, h: 22 });
    expect(pin!.y + pin!.h / 2).toBe(target.y + target.h / 2);
    expect(overlaps(pin!, stage)).toBe(false);
  });

  it("takes the right margin when the left one is out of the canvas", () => {
    const edge = { x: 0, y: 0, w: 220, h: 200 };
    const [pin] = placePins([target], { width: 300, height: 200 }, PIN_SIZE, {
      occupied: surrounded,
      stage: edge,
    });

    expect(pin).toEqual({ x: 226, y: 79, w: 22, h: 22 });
  });

  it("slides along the margin away from a pin already placed there", () => {
    const pins = placePins([target, { ...target }], bounds, PIN_SIZE, {
      occupied: surrounded,
      stage,
    });

    expect(pins[0]).toEqual({ x: 32, y: 79, w: 22, h: 22 });
    expect(pins[1]!.x).toBe(32);
    expect(pins[1]!.y).not.toBe(79);
    expect(overlaps(pins[1]!, { x: 30, y: 77, w: 26, h: 26 })).toBe(false);
  });

  it("prefers a clean side over the margin", () => {
    expect(placePins([target], bounds, PIN_SIZE, { occupied: [], stage })[0]).toEqual({
      x: 109,
      y: 52,
      w: 22,
      h: 22,
    });
  });

  it("falls back to the inside corner only when the margin is out of reach too", () => {
    const wide = { width: 200, height: 200 };
    const card = { x: 0, y: 60, w: 200, h: 80 };
    const full = { x: 0, y: 0, w: 200, h: 200 };
    const blockers = [
      { x: 0, y: 30, w: 200, h: 26 },
      { x: 0, y: 144, w: 200, h: 26 },
    ];

    const [pin] = placePins([card], wide, PIN_SIZE, { occupied: blockers, stage: full });

    expect(pin).toEqual({ x: 6, y: 66, w: 22, h: 22 });
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
