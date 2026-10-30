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

const RAIL_X = 4;
const CELL_W = 16;
const CURRENT_CELL = 4;
const DONE_CELLS = 4;

const faint = (fill: string, opacity: number) => ({ fill, opacity });
const stroked = (stroke: string, strokeWidth: number, opacity = 1) => ({
  fill: "none",
  stroke,
  strokeWidth,
  strokeLinecap: "round" as const,
  opacity,
});

function SwatchHeader({ palette }: { palette: ThemePalette }): ReactElement {
  return (
    <g>
      <rect width={SWATCH_SIZE.width} height="13" style={{ fill: palette.card }} />
      <path d="M0 13H120" style={stroked(palette.border, 0.5)} />
      <g
        transform="translate(5 3.6) scale(0.8)"
        style={{ ...stroked(palette.foreground, 0.5), strokeLinejoin: "miter" }}
      >
        {LOGO_PATHS.map((d) => (
          <path key={d} d={d} />
        ))}
      </g>
      <path d="M18 4.6H28" style={stroked(palette.mutedForeground, 1.2, 0.6)} />
      <path d="M18 8.4H34" style={stroked(palette.foreground, 1.8, 0.8)} />
      <rect
        x="62"
        y="3"
        width="30"
        height="7"
        rx="2.5"
        style={{ fill: palette.card, stroke: palette.border, strokeWidth: 0.5 }}
      />
      <rect x="63.2" y="4.2" width="8" height="4.6" rx="1.6" style={faint(palette.primary, 0.3)} />
      <path d="M74 6.5H79 M82 6.5H88" style={stroked(palette.mutedForeground, 1.2, 0.6)} />
      <circle cx="100" cy="6.5" r="2.8" style={faint(palette.foreground, 0.3)} />
      <circle cx="106.5" cy="6.5" r="2.8" style={faint(palette.foreground, 0.2)} />
    </g>
  );
}

function SwatchSteps({ palette }: { palette: ThemePalette }): ReactElement {
  const cells = Array.from({ length: 7 }, (_, index) => RAIL_X + index * CELL_W);
  return (
    <>
      <rect
        x={RAIL_X}
        y="17"
        width={CELL_W * 7}
        height="9"
        rx="2"
        style={{ fill: palette.card, stroke: palette.border, strokeWidth: 0.5 }}
      />
      <g data-region="steps">
        {cells.map((x, index) => (
          <rect
            key={x}
            x={x}
            y="17"
            width={CELL_W}
            height="9"
            style={
              index === CURRENT_CELL
                ? { fill: palette.primary, opacity: 0.3 }
                : { fill: "transparent" }
            }
          />
        ))}
        {cells.map((x, index) => (
          <g key={x}>
            <circle
              cx={x + 4}
              cy="21.2"
              r="1.5"
              style={
                index < DONE_CELLS
                  ? faint(palette.mutedForeground, 0.9)
                  : { ...stroked(palette.mutedForeground, 0.5, 0.7) }
              }
            />
            <path d={`M${x + 7.5} 20.4h5`} style={stroked(palette.foreground, 1.4, 0.55)} />
            <path
              d={`M${x + 3} 23.8h${CELL_W - 6}`}
              style={stroked(palette.mutedForeground, 0.9, 0.5)}
            />
          </g>
        ))}
        <path d={`M${cells[CURRENT_CELL]} 25.6h${CELL_W}`} style={stroked(palette.primary, 1.2)} />
      </g>
    </>
  );
}

function SwatchNextStep({ palette }: { palette: ThemePalette }): ReactElement {
  return (
    <g>
      <rect
        x="4"
        y="29"
        width="76"
        height="11"
        rx="2.5"
        style={{
          fill: "none",
          stroke: palette.border,
          strokeWidth: 0.6,
          strokeDasharray: "2 1.5",
        }}
      />
      <rect
        x="6.5"
        y="31.5"
        width="6"
        height="6"
        rx="1.5"
        style={faint(palette.foreground, 0.16)}
      />
      <path d="M15 33.4H36" style={stroked(palette.foreground, 1.4, 0.75)} />
      <path d="M15 36.4H46" style={stroked(palette.mutedForeground, 1, 0.55)} />
      <rect x="66" y="32" width="11" height="5" rx="1.5" style={{ fill: palette.primary }} />
    </g>
  );
}

function SwatchReading({ palette }: { palette: ThemePalette }): ReactElement {
  return (
    <g>
      <rect x="4" y="44" width="36" height="3.2" rx="1" style={faint(palette.foreground, 0.7)} />
      <path d="M4 51.5H74" style={stroked(palette.foreground, 1.6, 0.25)} />
      <path d="M4 56.1H70" style={stroked(palette.foreground, 1.6, 0.25)} />
      <path d="M4 60.7H46" style={stroked(palette.foreground, 1.6, 0.25)} />
      <rect x="14" y="54.4" width="34" height="3.4" style={faint(palette.primary, 0.25)} />
      <path d="M14 53.8V58.4 M48 53.8V58.4" style={stroked(palette.primary, 0.7)} />
      <rect x="50.4" y="54.9" width="2.2" height="2.2" style={faint(palette.foreground, 0.55)} />
      <rect
        x="53.8"
        y="54.9"
        width="2.2"
        height="2.2"
        style={faint(palette.mutedForeground, 0.7)}
      />
    </g>
  );
}

function SwatchChat({ palette }: { palette: ThemePalette }): ReactElement {
  return (
    <g>
      <rect
        x="84"
        y="29"
        width="32"
        height="43"
        rx="3"
        style={{ fill: palette.card, stroke: palette.border, strokeWidth: 0.6 }}
      />
      <rect x="87" y="32" width="26" height="12" rx="1.5" style={{ fill: palette.background }} />
      <path d="M87 32V44" style={stroked(palette.primary, 1.5)} />
      <path d="M91 36H108 M91 39.5H103" style={stroked(palette.foreground, 1.4, 0.25)} />
      <rect
        x="87"
        y="63"
        width="26"
        height="6"
        rx="1.5"
        style={{ fill: palette.background, stroke: palette.border, strokeWidth: 0.6 }}
      />
      <path d="M90 66H102" style={stroked(palette.mutedForeground, 1, 0.5)} />
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
      <SwatchNextStep palette={palette} />
      <SwatchReading palette={palette} />
      <SwatchChat palette={palette} />
    </svg>
  );
}
