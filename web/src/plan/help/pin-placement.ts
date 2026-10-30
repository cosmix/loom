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

/**
 * What the stage holds besides the targets. `occupied` are the text runs and controls a pin must
 * not cover; `stage` is the lifted component's box, so the canvas margin around it can hold a pin.
 */
export interface PinLayout {
  occupied?: readonly Box[];
  stage?: Box | null;
}

function overlapArea(a: Box, b: Box): number {
  const width = Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x));
  const height = Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y));
  return width * height;
}

/** A control group this much wider than tall reads as a bar (the spine's head, a row of buttons). */
const BAR_RATIO = 4;

function isBar(target: Box, size: number): boolean {
  return target.w >= size * 8 && target.w >= target.h * BAR_RATIO;
}

/**
 * The four sides in preference order. A pin to the left of a bar hangs off its far end, away from
 * the controls the note describes, so a bar tries below before left and right.
 */
function candidates(target: Box, size: number): Box[] {
  const x = target.x + (target.w - size) / 2;
  const y = target.y + (target.h - size) / 2;
  const above = { x, y: target.y - size - PIN_GAP, w: size, h: size };
  const left = { x: target.x - size - PIN_GAP, y, w: size, h: size };
  const below = { x, y: target.y + target.h + PIN_GAP, w: size, h: size };
  const right = { x: target.x + target.w + PIN_GAP, y, w: size, h: size };
  return isBar(target, size) ? [above, below, left, right] : [above, left, below, right];
}

/** The top-left corner inside a target big enough to hold a pin with a margin, else null. */
function insideCorner(target: Box, size: number): Box | null {
  const room = size + PIN_GAP * 2;
  if (target.w < room || target.h < room) return null;
  return { x: target.x + PIN_GAP, y: target.y + PIN_GAP, w: size, h: size };
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

/**
 * Positions in the canvas margin outside the stage, level with the target's vertical centre: left
 * of the stage first, then right. A spot taken by a placed pin slides along the margin to the next
 * free row, nearest first.
 */
function gutterSpots(target: Box, stage: Box, size: number, bounds: Bounds): Box[] {
  const centre = target.y + (target.h - size) / 2;
  const columns = [stage.x - size - PIN_GAP, stage.x + stage.w + PIN_GAP];
  const rows = [0, 1, -1, 2, -2, 3, -3].map((step) => centre + step * (size + PIN_GAP));
  return columns.flatMap((x) =>
    rows.map((y) => ({ x, y, w: size, h: size })).filter((spot) => insideBounds(spot, bounds)),
  );
}

function fallback(spots: readonly Box[], target: Box, obstacles: readonly Box[]): Box {
  const outsideTarget = spots.filter((spot) => overlapArea(spot, target) === 0);
  const choices = outsideTarget.length > 0 ? outsideTarget : spots;
  const score = (spot: Box) => obstacles.reduce((sum, box) => sum + overlapArea(spot, box), 0);
  // Strict comparison preserves the side order when overlap areas tie.
  return choices.reduce((best, spot) => (score(spot) < score(best) ? spot : best));
}

function overlapsAny(spot: Box, boxes: readonly Box[]): boolean {
  return boxes.some((box) => overlapArea(spot, box) > 0);
}

/** The other targets plus each placed pin with 2px of clearance for the next pin. */
function obstaclesFor(
  targets: readonly (Box | null)[],
  index: number,
  placed: readonly (Box | null)[],
): Box[] {
  const obstacles = targets.flatMap((box, other) => (box !== null && other !== index ? [box] : []));
  for (const pin of placed) {
    if (pin !== null) {
      obstacles.push({ x: pin.x - 2, y: pin.y - 2, w: pin.w + 4, h: pin.h + 4 });
    }
  }
  return obstacles;
}

export function placePins(
  targets: readonly (Box | null)[],
  bounds: { width: number; height: number },
  pinSize: number = PIN_SIZE,
  layout: PinLayout = {},
): (Box | null)[] {
  const occupied = layout.occupied ?? [];
  const placed: (Box | null)[] = [];
  for (const [index, target] of targets.entries()) {
    if (target === null) {
      placed.push(null);
      continue;
    }
    const obstacles = obstaclesFor(targets, index, placed);
    const spots = candidates(target, pinSize);
    const clear = (spot: Box) => !overlapsAny(spot, obstacles);
    const free = spots.find(
      (spot) =>
        insideBounds(spot, bounds) &&
        overlapArea(spot, target) === 0 &&
        clear(spot) &&
        !overlapsAny(spot, occupied),
    );
    // No side is clean: the canvas margin beside the stage covers nothing the note describes.
    const gutter = layout.stage
      ? gutterSpots(target, layout.stage, pinSize, bounds).find(clear)
      : undefined;
    // Every side is taken (stacked cards share their edges): sit inside the target's corner
    // rather than float at a clamped spot away from it.
    const inside = insideCorner(target, pinSize);
    placed.push(
      free ??
        gutter ??
        (inside !== null && insideBounds(inside, bounds) && clear(inside) ? inside : null) ??
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
