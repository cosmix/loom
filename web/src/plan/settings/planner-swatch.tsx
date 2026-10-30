import type { CSSProperties, ReactElement } from "react";

import { LOGO_PATHS } from "@/components/logo";
import { SWATCH_SIZE, type ThemePalette } from "@/components/theme-picker";

/** The spine's rows in workflow order: name length, and whether the step is finished. */
const SPINE_ROWS = [
  { name: 10, done: true },
  { name: 14, done: false },
  { name: 10, done: true },
  { name: 8, done: true },
  { name: 13, done: false },
  { name: 6, done: false },
];
const CURRENT_ROW = 1;
const SPINE_WIDTH = 30;
const READ_X = 36;

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

/** One spine row: its mark, then its name. The current row takes the accent mark and a bolder name. */
function SpineRow({
  palette,
  y,
  name,
  done,
  current,
}: {
  palette: ThemePalette;
  y: number;
  name: number;
  done: boolean;
  current: boolean;
}): ReactElement {
  let mark: CSSProperties = stroked(palette.mutedForeground, 0.5, 0.7);
  if (done) mark = faint(palette.mutedForeground, 0.9);
  if (current) mark = { fill: palette.primary, opacity: 1 };
  return (
    <g>
      <circle
        cx="6"
        cy={y}
        r={current ? 1.7 : 1.4}
        data-current={current || undefined}
        style={mark}
      />
      <path
        d={`M10 ${y}h${name}`}
        style={
          current
            ? stroked(palette.foreground, 1.6, 0.9)
            : stroked(palette.mutedForeground, 1.2, 0.55)
        }
      />
    </g>
  );
}

/** The spine column: the next step with its one primary button, then the rows, a hairline on its edge. */
function SwatchSpine({ palette }: { palette: ThemePalette }): ReactElement {
  return (
    <g data-region="spine">
      <path d={`M${SPINE_WIDTH} 13V76`} style={stroked(palette.border, 0.5)} />
      <path d="M4 20H24" style={stroked(palette.foreground, 1.3, 0.75)} />
      <path d="M4 23.2H19" style={stroked(palette.mutedForeground, 1, 0.55)} />
      <rect x="4" y="26" width="13" height="5" rx="1.5" style={{ fill: palette.primary }} />
      {SPINE_ROWS.map((row, index) => (
        <SpineRow
          key={index}
          palette={palette}
          y={38 + index * 6.4}
          name={row.name}
          done={row.done}
          current={index === CURRENT_ROW}
        />
      ))}
    </g>
  );
}

function SwatchReading({ palette }: { palette: ThemePalette }): ReactElement {
  const line = (y: number, end: number) => (
    <path d={`M${READ_X} ${y}H${end}`} style={stroked(palette.foreground, 1.6, 0.25)} />
  );
  return (
    <g>
      <path d={`M${READ_X} 20H${READ_X + 8}`} style={stroked(palette.mutedForeground, 1, 0.5)} />
      <rect
        x={READ_X}
        y="22.5"
        width="30"
        height="3.2"
        rx="1"
        style={faint(palette.foreground, 0.7)}
      />
      {line(31, 80)}
      {line(35.6, 76)}
      {line(40.2, 58)}
      <rect x={READ_X + 4} y="33.9" width="30" height="3.4" style={faint(palette.primary, 0.25)} />
      <path
        d={`M${READ_X + 4} 33.3V37.9 M${READ_X + 34} 33.3V37.9`}
        style={stroked(palette.primary, 0.7)}
      />
      <rect
        x={READ_X}
        y="46"
        width="22"
        height="2.6"
        rx="1"
        style={faint(palette.foreground, 0.55)}
      />
      {line(53, 80)}
      {line(57.6, 74)}
      {line(62.2, 78)}
      {line(66.8, 52)}
    </g>
  );
}

function SwatchChat({ palette }: { palette: ThemePalette }): ReactElement {
  return (
    <g>
      <rect
        x="84"
        y="17"
        width="32"
        height="55"
        rx="3"
        style={{ fill: palette.card, stroke: palette.border, strokeWidth: 0.6 }}
      />
      <rect x="87" y="20" width="26" height="12" rx="1.5" style={{ fill: palette.background }} />
      <path d="M87 20V32" style={stroked(palette.primary, 1.5)} />
      <path d="M91 24H108 M91 27.5H103" style={stroked(palette.foreground, 1.4, 0.25)} />
      <path d="M91 40H110 M91 43.5H100" style={stroked(palette.foreground, 1.4, 0.18)} />
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
      <SwatchSpine palette={palette} />
      <SwatchReading palette={palette} />
      <SwatchChat palette={palette} />
    </svg>
  );
}
