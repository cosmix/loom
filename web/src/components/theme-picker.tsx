import { useAtom } from "jotai";
import { Fragment, type ReactElement } from "react";
import {
  darkVariantAtom,
  lightVariantAtom,
  themeAtom,
  type ColorScheme,
  type DarkVariant,
  type LightVariant,
} from "@/aurora-ui/shared/atoms/theme";

export interface ThemePalette {
  background: string;
  card: string;
  foreground: string;
  mutedForeground: string;
  border: string;
  primary: string;
  primaryForeground: string;
}

export type ThemeId = "ledger" | "aubergine" | "pacific" | "graphite";

export interface ThemeOption {
  id: ThemeId;
  label: string;
  caption: string;
  scheme: ColorScheme;
  lightVariant: LightVariant | null;
  darkVariant: DarkVariant | null;
  palette: ThemePalette;
}

export const THEME_OPTIONS: readonly ThemeOption[] = [
  {
    id: "ledger",
    label: "Ledger",
    caption: "light · hue 102",
    scheme: "light",
    lightVariant: "ledger",
    darkVariant: null,
    palette: {
      background: "oklch(0.925 0.009 102)",
      card: "oklch(0.955 0.008 102)",
      foreground: "oklch(0.21 0.012 262)",
      mutedForeground: "oklch(0.48 0.02 250)",
      border: "oklch(0.84 0.012 100)",
      primary: "oklch(0.84 0.155 80)",
      primaryForeground: "oklch(0.21 0.012 262)",
    },
  },
  {
    id: "aubergine",
    label: "Aubergine",
    caption: "dark · hue 325",
    scheme: "dark",
    lightVariant: null,
    darkVariant: "aubergine",
    palette: {
      background: "oklch(0.16 0.035 325)",
      card: "oklch(0.195 0.04 325)",
      foreground: "oklch(0.94 0.012 325)",
      mutedForeground: "oklch(0.73 0.03 325)",
      border: "oklch(0.31 0.045 325)",
      primary: "oklch(0.83 0.165 82)",
      primaryForeground: "oklch(0.2 0.04 325)",
    },
  },
  {
    id: "pacific",
    label: "Pacific",
    caption: "dark blue · hue 258",
    scheme: "dark",
    lightVariant: null,
    darkVariant: "blue",
    palette: {
      background: "oklch(0.18 0.045 258)",
      card: "oklch(0.225 0.045 258)",
      foreground: "oklch(0.95 0.018 258)",
      mutedForeground: "oklch(0.76 0.04 258)",
      border: "oklch(0.42 0.06 258)",
      primary: "oklch(0.78 0.135 258)",
      primaryForeground: "oklch(0.17 0.045 258)",
    },
  },
  {
    id: "graphite",
    label: "Graphite",
    caption: "dark gray · hue 270",
    scheme: "dark",
    lightVariant: null,
    darkVariant: "gray",
    palette: {
      background: "oklch(0.18 0.005 270)",
      card: "oklch(0.225 0.005 270)",
      foreground: "oklch(0.95 0.005 270)",
      mutedForeground: "oklch(0.74 0.008 270)",
      border: "oklch(0.42 0.006 270)",
      primary: "oklch(0.78 0.015 270)",
      primaryForeground: "oklch(0.18 0.005 270)",
    },
  },
];

const EDGE_PATHS = [
  "M36 27 H43 Q46 27 46 30 V39 Q46 42 49 42",
  "M36 57 H43 Q46 57 46 54 V45 Q46 42 49 42",
  "M76 42 H81 Q84 42 84 39 V30 Q84 27 87 27",
  "M76 42 H81 Q84 42 84 45 V54 Q84 57 87 57",
] as const;

const ARROW_PATHS = [
  "M50 42 L46 40.2 L46 43.8 Z",
  "M88 27 L84 25.2 L84 28.8 Z",
  "M88 57 L84 55.2 L84 58.8 Z",
] as const;

interface StageCardSpec {
  x: number;
  y: number;
  width: number;
  labelWidth: number;
  labelOpacity: number;
  accent?: boolean;
}

const STAGE_CARDS: readonly StageCardSpec[] = [
  { x: 10, y: 21, width: 26, labelWidth: 14, labelOpacity: 0.55 },
  { x: 10, y: 51, width: 26, labelWidth: 14, labelOpacity: 0.55 },
  { x: 50, y: 36, width: 26, labelWidth: 14, labelOpacity: 0.8, accent: true },
  { x: 88, y: 21, width: 22, labelWidth: 12, labelOpacity: 0.4 },
  { x: 88, y: 51, width: 22, labelWidth: 12, labelOpacity: 0.4 },
];

function SwatchHeader({ palette }: { palette: ThemePalette }): ReactElement {
  return (
    <>
      <rect x="0" y="0" width="120" height="76" style={{ fill: palette.background }} />
      <rect x="0" y="0" width="120" height="13" style={{ fill: palette.card }} />
      <rect
        x="7"
        y="5"
        width="16"
        height="3"
        rx="1.5"
        style={{ fill: palette.foreground, opacity: 0.8 }}
      />
      <rect
        x="95"
        y="5"
        width="8"
        height="3"
        rx="1.5"
        style={{ fill: palette.mutedForeground, opacity: 0.55 }}
      />
      <rect
        x="106"
        y="5"
        width="8"
        height="3"
        rx="1.5"
        style={{ fill: palette.mutedForeground, opacity: 0.55 }}
      />
    </>
  );
}

function SwatchEdges({ palette }: { palette: ThemePalette }): ReactElement {
  return (
    <>
      <g
        style={{
          fill: "none",
          stroke: palette.mutedForeground,
          strokeWidth: 1.1,
          opacity: 0.7,
        }}
      >
        {EDGE_PATHS.map((d) => (
          <path key={d} d={d} />
        ))}
      </g>
      <g style={{ fill: palette.mutedForeground, opacity: 0.7 }}>
        {ARROW_PATHS.map((d) => (
          <path key={d} d={d} />
        ))}
      </g>
    </>
  );
}

function SwatchStageCard({
  card,
  palette,
}: {
  card: StageCardSpec;
  palette: ThemePalette;
}): ReactElement {
  const cardStyle = card.accent
    ? { fill: palette.primary }
    : { fill: palette.card, stroke: palette.border };
  const labelStyle = {
    fill: card.accent ? palette.primaryForeground : palette.foreground,
    opacity: card.labelOpacity,
  };

  return (
    <>
      <rect x={card.x} y={card.y} width={card.width} height="12" rx="3" style={cardStyle} />
      <rect
        x={card.x + 4}
        y={card.y + 4.5}
        width={card.labelWidth}
        height="3"
        rx="1.5"
        style={labelStyle}
      />
    </>
  );
}

function ThemeSwatch({ palette }: { palette: ThemePalette }): ReactElement {
  return (
    <svg
      viewBox="0 0 120 76"
      width="120"
      height="76"
      aria-hidden="true"
      style={{ display: "block", borderRadius: "5px" }}
    >
      <SwatchHeader palette={palette} />
      <SwatchEdges palette={palette} />
      {STAGE_CARDS.map((card) => (
        <Fragment key={`${card.x}-${card.y}`}>
          <SwatchStageCard card={card} palette={palette} />
        </Fragment>
      ))}
    </svg>
  );
}

export function ThemePicker(): ReactElement {
  const [scheme, setScheme] = useAtom(themeAtom);
  const [lightVariant, setLightVariant] = useAtom(lightVariantAtom);
  const [darkVariant, setDarkVariant] = useAtom(darkVariantAtom);

  return (
    <div role="group" aria-label="theme" className="flex flex-wrap gap-3">
      {THEME_OPTIONS.map((option) => {
        const active =
          option.scheme === scheme &&
          (option.scheme === "light"
            ? option.lightVariant === lightVariant
            : option.darkVariant === darkVariant);

        return (
          <button
            key={option.id}
            type="button"
            aria-pressed={active}
            aria-label={`${option.label} theme`}
            onClick={() => {
              setScheme(option.scheme);
              if (option.lightVariant !== null) setLightVariant(option.lightVariant);
              if (option.darkVariant !== null) setDarkVariant(option.darkVariant);
            }}
            className={`flex flex-col items-start gap-1 rounded-lg border bg-card p-2 text-left transition-shadow ${
              active ? "border-primary ring-2 ring-primary" : "border-border"
            }`}
          >
            <ThemeSwatch palette={option.palette} />
            <span className="mt-1 text-[13px] font-medium leading-4">{option.label}</span>
            <span className="text-xs leading-4 text-muted-foreground">{option.caption}</span>
          </button>
        );
      })}
    </div>
  );
}
