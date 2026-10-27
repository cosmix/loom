import { describe, expect, it } from "vitest";

import { placeControl } from "./floating-position";

const control = { width: 100, height: 30 };
const phone = { width: 320, height: 600 };

describe("placeControl", () => {
  it("sits below the selection, left-aligned with it", () => {
    expect(placeControl(new DOMRect(40, 100, 80, 20), control, phone)).toEqual({
      top: 126,
      left: 40,
    });
  });

  it("clamps left so the control stays inside the right edge", () => {
    const placed = placeControl(new DOMRect(250, 100, 60, 20), control, phone);
    expect(placed?.left).toBe(320 - 100 - 8);
  });

  it("clamps left to the margin when the selection starts off the left edge", () => {
    expect(placeControl(new DOMRect(-30, 100, 80, 20), control, phone)?.left).toBe(8);
  });

  it("keeps the left margin when the viewport is narrower than the control", () => {
    const narrow = { width: 90, height: 600 };
    expect(placeControl(new DOMRect(10, 100, 20, 20), control, narrow)?.left).toBe(8);
  });

  it("flips above the selection when there is no room below", () => {
    // Last visible line: 560-580 in a 600px viewport leaves 14px below.
    expect(placeControl(new DOMRect(40, 560, 80, 20), control, phone)).toEqual({
      top: 560 - 6 - 30,
      left: 40,
    });
  });

  it("clamps into the viewport when it fits neither above nor below", () => {
    const placed = placeControl(new DOMRect(40, 5, 80, 590), control, phone);
    expect(placed?.top).toBe(600 - 30 - 8);
  });

  it("returns null for a selection entirely outside the viewport", () => {
    expect(placeControl(new DOMRect(40, -80, 80, 20), control, phone)).toBeNull();
    expect(placeControl(new DOMRect(40, 700, 80, 20), control, phone)).toBeNull();
    expect(placeControl(new DOMRect(-200, 100, 80, 20), control, phone)).toBeNull();
    expect(placeControl(new DOMRect(400, 100, 80, 20), control, phone)).toBeNull();
  });

  it("still shows a selection that is partly visible", () => {
    expect(placeControl(new DOMRect(40, 590, 80, 40), control, phone)).not.toBeNull();
  });
});
