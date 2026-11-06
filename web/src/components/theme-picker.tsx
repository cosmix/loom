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

export const SWATCH_SIZE = { width: 120, height: 76 } as const;

export interface ThemePalette {
  background: string;
  card: string;
  foreground: string;
  mutedForeground: string;
  border: string;
  primary: string;
  primaryForeground: string;
}

export type ThemeId = "ledger" | "aubergine" | "walnut" | "graphite";

export interface ThemeOption {
  id: ThemeId;
  label: string;
  caption: string;
  scheme: ColorScheme;
  lightVariant: LightVariant | null;
  darkVariant: DarkVariant | null;
  palette: ThemePalette;
}

const PALETTE_VARIABLES: ThemePalette = {
  background: "var(--background)",
  card: "var(--card)",
  foreground: "var(--foreground)",
  mutedForeground: "var(--muted-foreground)",
  border: "var(--border)",
  primary: "var(--primary)",
  primaryForeground: "var(--primary-foreground)",
};

export const THEME_OPTIONS: readonly ThemeOption[] = [
  {
    id: "ledger",
    label: "Ledger",
    caption: "light · stone paper",
    scheme: "light",
    lightVariant: "ledger",
    darkVariant: null,
    palette: PALETTE_VARIABLES,
  },
  {
    id: "aubergine",
    label: "Aubergine",
    caption: "dark · hue 325",
    scheme: "dark",
    lightVariant: null,
    darkVariant: "aubergine",
    palette: PALETTE_VARIABLES,
  },
  {
    id: "walnut",
    label: "Walnut",
    caption: "dark · warm brown-black",
    scheme: "dark",
    lightVariant: null,
    darkVariant: "walnut",
    palette: PALETTE_VARIABLES,
  },
  {
    id: "graphite",
    label: "Graphite",
    caption: "dark · neutral charcoal",
    scheme: "dark",
    lightVariant: null,
    darkVariant: "gray",
    palette: PALETTE_VARIABLES,
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

function ThemeSwatch(palette: ThemePalette): ReactElement {
  return (
    <svg
      viewBox={`0 0 ${SWATCH_SIZE.width} ${SWATCH_SIZE.height}`}
      width={SWATCH_SIZE.width}
      height={SWATCH_SIZE.height}
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

export function ThemePicker({
  swatch = ThemeSwatch,
  optionClassName,
  themes,
}: {
  swatch?: (palette: ThemePalette) => ReactElement;
  /** Added to every option button, for a caller whose stylesheet restyles the cards. */
  optionClassName?: string;
  /** The themes to offer; every theme when omitted. */
  themes?: readonly ThemeId[];
} = {}): ReactElement {
  const [scheme, setScheme] = useAtom(themeAtom);
  const [lightVariant, setLightVariant] = useAtom(lightVariantAtom);
  const [darkVariant, setDarkVariant] = useAtom(darkVariantAtom);
  const options = themes
    ? THEME_OPTIONS.filter((option) => themes.includes(option.id))
    : THEME_OPTIONS;

  return (
    <div role="group" aria-label="theme" className="flex flex-wrap gap-3">
      {options.map((option) => {
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
            className={`flex flex-col items-start gap-1 rounded-lg border bg-card p-2 text-left transition-shadow focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring ${
              active ? "border-primary ring-2 ring-primary" : "border-border"
            }${optionClassName ? ` ${optionClassName}` : ""}`}
          >
            <span
              className={option.scheme === "dark" ? "theme-preview dark" : "theme-preview"}
              data-light-theme={option.lightVariant ?? undefined}
              data-dark-theme={option.darkVariant ?? undefined}
            >
              {swatch(option.palette)}
            </span>
            <span className="mt-1 text-[13px] font-medium leading-4">{option.label}</span>
            <span className="text-xs leading-4 text-muted-foreground">{option.caption}</span>
          </button>
        );
      })}
    </div>
  );
}
