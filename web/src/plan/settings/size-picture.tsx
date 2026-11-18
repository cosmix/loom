import type { ReactElement } from "react";

import type { ReadingSize } from "./preferences";

/** A text line in the little page: its width, drawn as a rounded bar. */
type Line = { width: number };

/** Every bar starts here: 3.5px inside the outline's left edge (4.5). */
const LINE_X = 8;

/**
 * The page's lines per reading size, in the 40 by 32 box. Compact packs five thin lines, Comfortable
 * four, Large three thick ones headed by a short first bar; each set keeps the same 3.5px margin
 * inside the page outline, so the tiles read as the same page at three settings.
 */
const PAGES: Record<ReadingSize, { height: number; gap: number; lines: Line[] }> = {
  17: {
    height: 2,
    gap: 2,
    lines: [{ width: 24 }, { width: 24 }, { width: 20 }, { width: 24 }, { width: 16 }],
  },
  19: {
    height: 2.5,
    gap: 2.5,
    lines: [{ width: 24 }, { width: 22 }, { width: 24 }, { width: 14 }],
  },
  21: {
    height: 3,
    gap: 3.5,
    lines: [{ width: 14 }, { width: 24 }, { width: 20 }],
  },
};

/**
 * A page outline with its text as bars, in `currentColor`, so the tile's colour states (muted,
 * hovered, chosen in the accent ink) colour the drawing without a second rule.
 */
export function SizePicture({ size }: { size: ReadingSize }): ReactElement {
  const { height, gap, lines } = PAGES[size];
  const block = lines.length * height + (lines.length - 1) * gap;
  const top = (32 - block) / 2;
  return (
    <svg width="40" height="32" viewBox="0 0 40 32" aria-hidden="true" focusable="false">
      <rect
        data-page=""
        x="4.5"
        y="0.5"
        width="31"
        height="31"
        rx="3"
        fill="none"
        stroke="currentColor"
      />
      {lines.map((line, index) => (
        <rect
          key={index}
          data-line=""
          x={LINE_X}
          y={top + index * (height + gap)}
          width={line.width}
          height={height}
          rx={height / 2}
          fill="currentColor"
        />
      ))}
    </svg>
  );
}
