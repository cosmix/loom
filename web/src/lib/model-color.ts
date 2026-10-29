const LIGHTNESS = [0.6, 0.72, 0.84] as const;
const CHROMA = 0.16;
/// Hue steps tried when a model's own hue is too close to one already in use.
const HUE_STEP = 15;
/// Minimum `distance` between two models' colors before one is nudged.
const MIN_DISTANCE = 45;
/// Degrees of hue a full unit of lightness is worth in `distance`.
const LIGHTNESS_WEIGHT = 250;
const STORAGE_KEY = "loom.model-colors";

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
  return { hue: h % 360, lightness: LIGHTNESS[Math.floor(h / 360) % LIGHTNESS.length]! };
}

export function swatchColor({ hue, lightness }: ModelSwatch): string {
  return `oklch(${lightness} ${CHROMA} ${hue})`;
}

/// A model's chart color from its name alone, with no regard for other models.
export function modelColor(model: string): string {
  return swatchColor(preferredSwatch(model));
}

/// Perceptual-ish gap between two swatches: the shorter way round the hue
/// circle, combined with lightness scaled to the same unit.
export function distance(a: ModelSwatch, b: ModelSwatch): number {
  const gap = Math.abs(a.hue - b.hue) % 360;
  return Math.hypot(Math.min(gap, 360 - gap), (a.lightness - b.lightness) * LIGHTNESS_WEIGHT);
}

function nearest(swatch: ModelSwatch, taken: readonly ModelSwatch[]): number {
  let min = Infinity;
  for (const other of taken) min = Math.min(min, distance(swatch, other));
  return min;
}

/// `preferred`, or the closest candidate to it that sits at least MIN_DISTANCE
/// from every swatch in `taken`. Candidates walk outward in hue (+15, -15, +30,
/// ...) at each lightness step, nearest first, so the result is deterministic
/// and stays near the preferred color. When nothing clears the bar, the
/// candidate farthest from its nearest neighbour wins.
export function spread(preferred: ModelSwatch, taken: readonly ModelSwatch[]): ModelSwatch {
  if (taken.length === 0 || nearest(preferred, taken) >= MIN_DISTANCE) return preferred;
  let best = preferred;
  let bestGap = nearest(preferred, taken);
  const steps = 360 / HUE_STEP;
  for (let step = 1; step <= steps / 2; step++) {
    for (const sign of [1, -1]) {
      const hue = (((preferred.hue + sign * step * HUE_STEP) % 360) + 360) % 360;
      const lightnesses = [
        preferred.lightness,
        ...LIGHTNESS.filter((l) => l !== preferred.lightness),
      ];
      for (const lightness of lightnesses) {
        const candidate = { hue, lightness };
        const gap = nearest(candidate, taken);
        if (gap >= MIN_DISTANCE) return candidate;
        if (gap > bestGap) {
          best = candidate;
          bestGap = gap;
        }
      }
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
      Object.entries(raw).filter((entry): entry is [string, ModelSwatch] => isSwatch(entry[1])),
    );
  } catch {
    return new Map();
  }
}

function save(swatches: ReadonlyMap<string, ModelSwatch>): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(Object.fromEntries(swatches)));
  } catch {
    // Storage can be blocked or full; colors then stay stable for this page only.
  }
}

let known: Map<string, ModelSwatch> | undefined;

/// The page's model colors. Swatches are remembered for the page and in
/// localStorage, so a model keeps its color across renders, new models and
/// reloads in this browser. A model seen for the first time is spread away from
/// the swatches already given out.
export function stickyModelColors(models: readonly string[]): Map<string, string> {
  known ??= load();
  const fresh = models.filter((model) => !known!.has(model));
  if (fresh.length > 0) {
    for (const [model, swatch] of assignSwatches(models, known)) known.set(model, swatch);
    save(known);
  }
  return new Map(models.map((model) => [model, swatchColor(known!.get(model)!)]));
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
