import type { ReactElement } from "react";

import { SWATCH_SIZE, type ThemePalette } from "@/components/theme-picker";

// Miniature of the terminal logo strokes in components/logo.tsx.
const LOGO_PATHS = [
  "M0.5 1V7",
  "M0 7H2.5V6",
  "M3.5 3H5.5V7H3.5Z",
  "M6.5 3H8.5V7H6.5Z",
  "M9.5 7V3H11.5V7",
  "M10.5 3V6",
  "M9 7H10",
  "M11 7H12",
] as const;

const VIEW_MARKS = [82, 91, 100, 109] as const;
const STEP_POSITIONS = [6, 22, 38, 54, 70, 86, 102] as const;

function SwatchHeader({ palette }: { palette: ThemePalette }): ReactElement {
  return (
    <g>
      <rect width={SWATCH_SIZE.width} height="13" style={{ fill: palette.card }} />
      <g
        transform="translate(6 2.5) scale(0.95)"
        style={{
          fill: "none",
          stroke: palette.foreground,
          strokeWidth: 0.42,
          strokeLinecap: "round",
          strokeLinejoin: "miter",
        }}
      >
        {LOGO_PATHS.map((d) => (
          <path key={d} d={d} />
        ))}
      </g>
      <rect
        x="79"
        y="3"
        width="35"
        height="7"
        rx="3.5"
        style={{ fill: palette.background, stroke: palette.border, strokeWidth: 0.6 }}
      />
      <rect x="80" y="4" width="8" height="5" rx="2.5" style={{ fill: palette.border }} />
      <path d="M88 4V9 M97 4V9 M106 4V9" style={{ stroke: palette.border, strokeWidth: 0.5 }} />
      {VIEW_MARKS.map((x) => (
        <rect
          key={x}
          x={x}
          y="5.5"
          width="3"
          height="2"
          rx="0.5"
          style={{ fill: palette.foreground, opacity: 0.5 }}
        />
      ))}
    </g>
  );
}

function SwatchSteps({ palette }: { palette: ThemePalette }): ReactElement {
  return (
    <g data-region="steps">
      {STEP_POSITIONS.map((x, index) => (
        <rect
          key={x}
          x={x}
          y="17"
          width="12"
          height="6"
          rx="1.5"
          style={{
            fill: index === 3 ? palette.primary : palette.card,
            stroke: index === 3 ? "none" : palette.border,
            strokeWidth: 0.5,
          }}
        />
      ))}
      <path
        d="M57 25H63"
        style={{ stroke: palette.primary, strokeWidth: 1, strokeLinecap: "round" }}
      />
    </g>
  );
}

function SwatchReading({ palette }: { palette: ThemePalette }): ReactElement {
  return (
    <g>
      <rect
        x="6"
        y="36"
        width="60"
        height="5"
        rx="1"
        style={{ fill: palette.foreground, opacity: 0.12 }}
      />
      <rect
        x="6"
        y="45"
        width="54"
        height="5"
        rx="1"
        style={{ fill: palette.foreground, opacity: 0.12 }}
      />
      <rect
        x="6"
        y="57"
        width="29"
        height="3"
        rx="1"
        style={{ fill: palette.foreground, opacity: 0.08 }}
      />
      <path d="M6 35V42 M60 44V51" style={{ stroke: palette.foreground, opacity: 0.45 }} />
    </g>
  );
}

function SwatchChat({ palette }: { palette: ThemePalette }): ReactElement {
  return (
    <g>
      <rect
        x="76"
        y="30"
        width="39"
        height="40"
        rx="3"
        style={{ fill: palette.card, stroke: palette.border, strokeWidth: 0.6 }}
      />
      <rect x="80" y="35" width="31" height="11" rx="1.5" style={{ fill: palette.background }} />
      <path d="M80 35V46" style={{ stroke: palette.primary, strokeWidth: 1.5 }} />
      <path
        d="M84 39H106 M84 42H101"
        style={{ stroke: palette.foreground, strokeWidth: 1.5, opacity: 0.25 }}
      />
      <rect
        x="80"
        y="59"
        width="31"
        height="7"
        rx="1.5"
        style={{ fill: palette.background, stroke: palette.border, strokeWidth: 0.6 }}
      />
      <path
        d="M83 62.5H98"
        style={{ stroke: palette.mutedForeground, strokeWidth: 1, opacity: 0.5 }}
      />
    </g>
  );
}

export function PlannerThemeSwatch(palette: ThemePalette): ReactElement {
  return (
    <svg
      viewBox={`0 0 ${SWATCH_SIZE.width} ${SWATCH_SIZE.height}`}
      width={SWATCH_SIZE.width}
      height={SWATCH_SIZE.height}
      aria-hidden="true"
      style={{ display: "block", borderRadius: "5px" }}
    >
      <rect
        width={SWATCH_SIZE.width}
        height={SWATCH_SIZE.height}
        style={{ fill: palette.background }}
      />
      <SwatchHeader palette={palette} />
      <SwatchSteps palette={palette} />
      <SwatchReading palette={palette} />
      <SwatchChat palette={palette} />
    </svg>
  );
}
