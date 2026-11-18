import { vi } from "vitest";

/** A `matchMedia` whose answer is `matches(query)`; jsdom has none, which reads as no motion. */
export function stubMotion(matches: (query: string) => boolean) {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: matches(query),
    media: query,
    onchange: null,
    addEventListener() {},
    removeEventListener() {},
    addListener() {},
    removeListener() {},
    dispatchEvent: () => false,
  }));
}
