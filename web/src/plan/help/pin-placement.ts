export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

export const PIN_SIZE = 22;
export const PIN_GAP = 6;
export const CANVAS_MARGIN = 32;

type Bounds = { width: number; height: number };

function overlapArea(a: Box, b: Box): number {
  const width = Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x));
  const height = Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y));
  return width * height;
}

function candidates(target: Box, size: number): Box[] {
  const x = target.x + (target.w - size) / 2;
  const y = target.y + (target.h - size) / 2;
  return [
    { x, y: target.y - size - PIN_GAP, w: size, h: size },
    { x: target.x - size - PIN_GAP, y, w: size, h: size },
    { x, y: target.y + target.h + PIN_GAP, w: size, h: size },
    { x: target.x + target.w + PIN_GAP, y, w: size, h: size },
  ];
}

function insideBounds(box: Box, bounds: Bounds): boolean {
  return (
    box.x >= 0 && box.y >= 0 && box.x + box.w <= bounds.width && box.y + box.h <= bounds.height
  );
}

function clampToBounds(box: Box, bounds: Bounds): Box {
  return {
    ...box,
    x: Math.max(0, Math.min(box.x, bounds.width - box.w)),
    y: Math.max(0, Math.min(box.y, bounds.height - box.h)),
  };
}

function fallback(spots: readonly Box[], target: Box, obstacles: readonly Box[]): Box {
  const outsideTarget = spots.filter((spot) => overlapArea(spot, target) === 0);
  const choices = outsideTarget.length > 0 ? outsideTarget : spots;
  const score = (spot: Box) => obstacles.reduce((sum, box) => sum + overlapArea(spot, box), 0);
  // Strict comparison preserves the side order when overlap areas tie.
  return choices.reduce((best, spot) => (score(spot) < score(best) ? spot : best));
}

export function placePins(
  targets: readonly (Box | null)[],
  bounds: { width: number; height: number },
  pinSize: number = PIN_SIZE,
): (Box | null)[] {
  const placed: (Box | null)[] = [];
  for (const [index, target] of targets.entries()) {
    if (target === null) {
      placed.push(null);
      continue;
    }
    const obstacles = targets.flatMap((box, other) =>
      box !== null && other !== index ? [box] : [],
    );
    // Reserve 2px around each placed pin for the next pin's clearance.
    for (const pin of placed) {
      if (pin !== null) {
        obstacles.push({ x: pin.x - 2, y: pin.y - 2, w: pin.w + 4, h: pin.h + 4 });
      }
    }
    const spots = candidates(target, pinSize);
    const free = spots.find(
      (spot) =>
        insideBounds(spot, bounds) &&
        overlapArea(spot, target) === 0 &&
        obstacles.every((box) => overlapArea(spot, box) === 0),
    );
    placed.push(
      free ??
        fallback(
          spots.map((spot) => clampToBounds(spot, bounds)),
          target,
          obstacles,
        ),
    );
  }
  return placed;
}

export function anchorBox(boxes: readonly Box[]): Box | null {
  if (boxes.length === 0) return null;
  return boxes.reduce((union, box) => {
    const x = Math.min(union.x, box.x);
    const y = Math.min(union.y, box.y);
    return {
      x,
      y,
      w: Math.max(union.x + union.w, box.x + box.w) - x,
      h: Math.max(union.y + union.h, box.y + box.h) - y,
    };
  });
}
