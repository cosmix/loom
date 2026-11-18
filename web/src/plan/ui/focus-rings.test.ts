import { expect, it } from "vitest";

/**
 * One focus sign per focused thing (DESIGN.md, Focus and hover). A box around words (the chat
 * composer, the kickoff field) draws the 2px ring off its resting edge while the words have
 * focus; it never also recolours its edge to the ring, which drew a second ring inside the first,
 * and no edge turns to the ring on `:focus-within` over a control that draws its own ring.
 */
const sheets = import.meta.glob("../**/*.css", { query: "?raw", import: "default", eager: true });

function blocks(css: string): { selector: string; body: string }[] {
  const stripped = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const found: { selector: string; body: string }[] = [];
  const rule = /([^{}]+)\{([^{}]*)\}/g;
  for (let match = rule.exec(stripped); match !== null; match = rule.exec(stripped)) {
    found.push({ selector: match[1]!.trim(), body: match[2]! });
  }
  return found;
}

it("draws one ring on a focused box: no edge turns to the ring beside or under an outline", () => {
  const offenders: string[] = [];
  for (const [path, css] of Object.entries(sheets)) {
    for (const block of blocks(css as string)) {
      const edge = /border(?:-[a-z-]+)?-color:\s*var\(--ring\)/.test(block.body);
      if (!edge) continue;
      const ring = /outline:\s*[^;]*var\(--ring\)/.test(block.body);
      const within = block.selector.includes(":focus-within");
      if (ring || within) offenders.push(`${path}: ${block.selector}`);
    }
  }
  expect(offenders).toEqual([]);
});
