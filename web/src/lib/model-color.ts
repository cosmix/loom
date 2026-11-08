const LIGHTNESS = [0.6, 0.72, 0.84] as const;
const CHROMA = 0.16;
/// Hue step of the candidate grid a crowded model is moved onto.
const HUE_STEP = 5;
/// Gap (OKLab distance) a model's color should keep from every other model's.
/// A hue gap of about 65 degrees at one lightness, or one lightness step plus a
/// hue gap of about 45 degrees. Placement stops caring once this is met, so a
/// model stays near its hash color; below it, the farthest candidate wins.
const TARGET_GAP = 0.18;
const STORAGE_KEY = "loom.model-colors.v2";

export interface ModelSwatch {
  hue: number;
  lightness: number;
}

/// FNV-1a over the UTF-16 code units of `text`, then the murmur3 finalizer.
/// FNV alone barely moves the low bits between names that differ in one
/// character ("opus-5-5" and "sonnet-5-5" would get near hues); the finalizer
/// spreads them. Unsigned 32-bit.
function hash(text: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  h ^= h >>> 16;
  h = Math.imul(h, 0x85ebca6b);
  h ^= h >>> 13;
  h = Math.imul(h, 0xc2b2ae35);
  h ^= h >>> 16;
  return h >>> 0;
}

/// The color a model prefers, from its name alone: the hash picks the hue and,
/// from a second slice, one of three lightness steps.
export function preferredSwatch(model: string): ModelSwatch {
  const h = hash(model);
  return {
    hue: h % 360,
    lightness: LIGHTNESS[Math.floor(h / 360) % LIGHTNESS.length]!,
  };
}

export function swatchColor({ hue, lightness }: ModelSwatch): string {
  return `oklch(${lightness} ${CHROMA} ${hue})`;
}

/// A model's chart color from its name alone, with no regard for other models.
export function modelColor(model: string): string {
  return swatchColor(preferredSwatch(model));
}

/// Gap between two swatches as the distance between their OKLab points: the
/// hue circle becomes a chord at the shared chroma and lightness adds in the
/// same unit, so "far apart on the color disc" means one thing for both axes.
export function distance(a: ModelSwatch, b: ModelSwatch): number {
  const rad = Math.PI / 180;
  return Math.hypot(
    a.lightness - b.lightness,
    CHROMA * (Math.cos(a.hue * rad) - Math.cos(b.hue * rad)),
    CHROMA * (Math.sin(a.hue * rad) - Math.sin(b.hue * rad)),
  );
}

function nearest(swatch: ModelSwatch, taken: readonly ModelSwatch[]): number {
  let min = Infinity;
  for (const other of taken) min = Math.min(min, distance(swatch, other));
  return min;
}

const GRID: readonly ModelSwatch[] = LIGHTNESS.flatMap((lightness) =>
  Array.from({ length: 360 / HUE_STEP }, (_, index) => ({
    hue: index * HUE_STEP,
    lightness,
  })),
);

/// `preferred`, or the best swatch in its place when it crowds a neighbour.
/// Candidates are `preferred` plus a grid of every hue step at every lightness.
/// Each is scored by its gap to the nearest swatch in `taken`, capped at
/// TARGET_GAP; the highest score wins and ties go to the candidate closest to
/// `preferred`. A model with room keeps its hash color, a crowded one moves to
/// the nearest spot with room, and when no spot has room the farthest-from-all
/// spot wins (farthest-point placement, so the palette fills the disc evenly).
/// Deterministic: equal scores resolve by candidate order.
export function spread(
  preferred: ModelSwatch,
  taken: readonly ModelSwatch[],
): ModelSwatch {
  if (taken.length === 0 || nearest(preferred, taken) >= TARGET_GAP)
    return preferred;
  let best = preferred;
  let bestScore = nearest(preferred, taken);
  let bestPull = 0;
  for (const candidate of GRID) {
    const score = Math.min(nearest(candidate, taken), TARGET_GAP);
    const pull = distance(candidate, preferred);
    if (
      score > bestScore + 1e-9 ||
      (score > bestScore - 1e-9 && pull < bestPull)
    ) {
      best = candidate;
      bestScore = score;
      bestPull = pull;
    }
  }
  return best;
}

/// Colors for `models`. A model in `assigned` keeps its swatch. The others, in
/// name order, start from their preferred swatch and are nudged away from every
/// swatch already given out. Adding a model therefore never recolors one that
/// already has a swatch.
export function assignSwatches(
  models: readonly string[],
  assigned: ReadonlyMap<string, ModelSwatch>,
): Map<string, ModelSwatch> {
  const result = new Map<string, ModelSwatch>();
  for (const model of models) {
    const swatch = assigned.get(model);
    if (swatch) result.set(model, swatch);
  }
  const taken = [...assigned.values()];
  for (const model of [...models].sort()) {
    if (result.has(model)) continue;
    const swatch = spread(preferredSwatch(model), taken);
    taken.push(swatch);
    result.set(model, swatch);
  }
  return result;
}

function isSwatch(value: unknown): value is ModelSwatch {
  if (typeof value !== "object" || value === null) return false;
  const { hue, lightness } = value as Record<string, unknown>;
  return typeof hue === "number" && typeof lightness === "number";
}

function load(): Map<string, ModelSwatch> {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    if (typeof raw !== "object" || raw === null) return new Map();
    return new Map(
      Object.entries(raw).filter((entry): entry is [string, ModelSwatch] =>
        isSwatch(entry[1]),
      ),
    );
  } catch {
    return new Map();
  }
}

function save(swatches: ReadonlyMap<string, ModelSwatch>): void {
  try {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify(Object.fromEntries(swatches)),
    );
  } catch {
    // Storage can be blocked or full; colors then stay stable for this page only.
  }
}

let known: Map<string, ModelSwatch> | undefined;

/// The page's model colors. Swatches are remembered for the page and in
/// localStorage, so a model keeps its color across renders, new models and
/// reloads in this browser. A model seen for the first time is spread away from
/// the swatches already given out.
export function stickyModelColors(
  models: readonly string[],
): Map<string, string> {
  known ??= load();
  const fresh = models.filter((model) => !known!.has(model));
  if (fresh.length > 0) {
    for (const [model, swatch] of assignSwatches(models, known))
      known.set(model, swatch);
    save(known);
  }
  return new Map(
    models.map((model) => [model, swatchColor(known!.get(model)!)]),
  );
}

/// Forgets every remembered swatch; tests use it to start from a clean page.
export function resetModelColors(): void {
  known = undefined;
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    // Nothing stored to forget.
  }
}
