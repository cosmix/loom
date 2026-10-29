const LIGHTNESS = [0.6, 0.72, 0.84] as const;

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

/// A model's chart color, derived from its name alone. It does not depend on
/// which other models are present or on their shares, so a model keeps its
/// color as new models appear in the plan or ship later. The hue comes from the
/// hash, and a second slice of the hash picks one of three lightness steps so
/// two names that land on near hues still read apart. A collision is still
/// possible; the legend and the table name every model, so color is never the
/// only identifier.
export function modelColor(model: string): string {
  const h = hash(model);
  const lightness = LIGHTNESS[Math.floor(h / 360) % LIGHTNESS.length];
  return `oklch(${lightness} 0.16 ${h % 360})`;
}
