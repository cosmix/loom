import { cn } from "cn";
import { useAtomValue } from "jotai/react";
import { InfoIcon } from "lucide-react";
import { useId } from "react";

import type { StageFlow, TokenFlow, TokenUsage } from "@/api/schema";
import { TokenModels } from "@/components/token-models";
import { compactTokens, fresh, lastValues, maxOf } from "@/lib/tokens";
import { snapshotAtom } from "@/state/atoms";

/// The chart's plot height in pixels; its width follows the container.
const PLOT_HEIGHT = 96;
const POINTS = 60;
const TICKS = [0, 0.5, 1] as const;
const AXIS_LABEL = "absolute text-[11px] leading-none text-muted-foreground";
const TIME_LABELS = [
  { text: "−30m", position: "left-0" },
  { text: "−15m", position: "left-1/2 -translate-x-1/2" },
  { text: "now", position: "right-0" },
] as const;

/// Stage colours: identity hues apart from the state tones, so no band reads
/// as the executing stage.
const PALETTE = [
  "var(--series-1)",
  "var(--series-2)",
  "var(--series-3)",
  "var(--series-4)",
  "var(--series-5)",
  "var(--series-6)",
] as const;

const MIX: ReadonlyArray<{
  key: keyof TokenUsage;
  label: string;
  color: string;
  hint?: string;
}> = [
  {
    key: "input",
    label: "uncached input",
    color: "var(--tone-executing)",
    hint: "Input tokens neither read from nor written to the prompt cache. Claude Code caches almost every prompt, so this stays at a few tokens per API call.",
  },
  { key: "cache_write", label: "cache write", color: "var(--tone-warning)" },
  { key: "output", label: "output", color: "var(--tone-completed)" },
  { key: "cache_read", label: "cache read", color: "var(--tone-dimmed)" },
];

/// The plan's token flow: the fresh rate, a stacked chart of each stage's last
/// 30 minutes, the token mix and the by-model breakdown. Renders nothing while
/// the snapshot carries no token flow.
export function TokenFlowPanel() {
  const status = useAtomValue(snapshotAtom)?.status;
  const flow = status?.tokens;
  if (!status || !flow) return null;
  const colors = stageColors(
    status.stages.map((stage) => stage.id),
    flow.stages,
  );
  return (
    <section aria-label="Token flow" className="flex min-w-0 flex-col gap-3">
      <div className="flex items-baseline justify-between gap-2">
        <h2 className="eyebrow">Token flow</h2>
        <span className="text-xs text-muted-foreground">5 min avg</span>
      </div>
      <div className="flex flex-col gap-4 rounded-lg border border-hairline bg-card/60 p-4 shadow-xs">
        <div className="flex items-baseline gap-2">
          <span className="font-mono text-3xl leading-none font-normal tabular-nums">
            {compactTokens(fresh(flow.plan.rate))}
          </span>
          <span className="text-sm text-muted-foreground">fresh / min</span>
        </div>
        <StageChart stages={flow.stages} colors={colors} />
        <Mix flow={flow} />
        <TokenModels flow={flow} />
      </div>
    </section>
  );
}

/// Each stage's colour, by its place among all the plan's stage ids in id
/// order: a stage keeps its colour while other stages start, finish or leave
/// the window.
function stageColors(planIds: string[], stages: StageFlow[]): Map<string, string> {
  const ids = [...new Set([...planIds, ...stages.map((stage) => stage.stage_id)])].sort();
  return new Map(ids.map((id, index) => [id, PALETTE[index % PALETTE.length]]));
}

/// Where a chart value sits in the 100 x 100 plot box, as a percentage.
const plotX = (index: number) => (index / (POINTS - 1)) * 100;
const plotY = (value: number, max: number) => 100 - (value / max) * 100;

/// One stage's layer: its `id`, the svg `points` outlining it, and `edge`, the
/// upper points alone. `empty` marks a stage with no tokens in the window.
interface Band {
  id: string;
  points: string;
  edge: string;
  empty: boolean;
}

/// One band per stage, stacked in `stages` order, the chart's top (the tallest
/// stacked cell, at least 1) and `last`, the stacked total at the newest point.
function stackBands(stages: StageFlow[]): { bands: Band[]; max: number; last: number } {
  const base = Array.from({ length: POINTS }, () => 0);
  const layers = stages.map((stage) => {
    const lower = [...base];
    const values = lastValues(stage.series, POINTS);
    values.forEach((value, index) => {
      base[index] = (base[index] ?? 0) + value;
    });
    return { id: stage.stage_id, lower, upper: [...base], empty: values.every((v) => v === 0) };
  });
  const max = maxOf(base, 1);
  const point = (value: number, index: number) =>
    `${plotX(index).toFixed(2)},${plotY(value, max).toFixed(2)}`;
  const bands = layers.map(({ id, lower, upper, empty }) => ({
    id,
    points: [...upper.map(point), ...lower.map(point).reverse()].join(" "),
    edge: upper.map(point).join(" "),
    empty,
  }));
  return { bands, max, last: base[POINTS - 1] ?? 0 };
}

function StageChart({ stages, colors }: { stages: StageFlow[]; colors: Map<string, string> }) {
  const { bands, max, last } = stackBands(stages);
  return (
    <div className="flex flex-col gap-2">
      {/* The bands are drawn in a stretched 100 x 100 box at a fixed pixel
          height; every label is HTML positioned by percentage, so text keeps
          its size at any width instead of scaling with the svg. */}
      <div className="relative pt-5 pb-5 pl-10">
        <span className="absolute top-0 left-0 text-[11px] leading-none text-muted-foreground">
          per 30 s
        </span>
        <div className="relative">
          <BandPlot bands={bands} max={max} colors={colors} />
          <AxisLabels max={max} />
          {last > 0 && (
            <span
              aria-hidden="true"
              className="absolute right-0 size-2 translate-x-1/2 -translate-y-1/2 rounded-full bg-foreground ring-2 ring-card"
              style={{ top: `${plotY(last, max)}%` }}
            />
          )}
        </div>
      </div>
      <StageLegend stages={stages} colors={colors} />
    </div>
  );
}

function BandPlot({
  bands,
  max,
  colors,
}: {
  bands: Band[];
  max: number;
  colors: Map<string, string>;
}) {
  const prefix = useId().replace(/:/g, "");
  const drawn = bands.map((band, index) => ({ band, index })).filter(({ band }) => !band.empty);
  return (
    <svg
      role="img"
      aria-label="fresh tokens per stage, last 30 min"
      width="100%"
      height={PLOT_HEIGHT}
      viewBox="0 0 100 100"
      preserveAspectRatio="none"
      className="block overflow-visible"
    >
      <defs>
        {drawn.map(({ band, index }) => (
          <linearGradient key={band.id} id={`${prefix}-${index}`} x1={0} y1={0} x2={0} y2={1}>
            <stop offset={0} stopColor={colors.get(band.id)} stopOpacity={0.85} />
            <stop offset={1} stopColor={colors.get(band.id)} stopOpacity={0.12} />
          </linearGradient>
        ))}
      </defs>
      <GridLines max={max} />
      {drawn.map(({ band, index }) => (
        <polygon
          key={band.id}
          points={band.points}
          fill={`url(#${prefix}-${index})`}
          stroke="none"
        />
      ))}
      {/* Reverse stack order: where an upper band is zero-thick, the lower edge wins. */}
      {[...drawn].reverse().map(({ band }) => (
        <polyline
          key={band.id}
          points={band.edge}
          fill="none"
          stroke={colors.get(band.id)}
          strokeWidth={1.5}
          strokeLinejoin="round"
          strokeLinecap="round"
          vectorEffect="non-scaling-stroke"
        />
      ))}
    </svg>
  );
}

/// The baseline is a solid hairline; the upper ticks are dashed and fainter.
function GridLines({ max }: { max: number }) {
  return TICKS.map((tick) => (
    <line
      key={tick}
      x1={0}
      x2={100}
      y1={plotY(max * tick, max)}
      y2={plotY(max * tick, max)}
      stroke="var(--hairline)"
      strokeWidth={1}
      strokeDasharray={tick === 0 ? undefined : "2 4"}
      strokeOpacity={tick === 0 ? undefined : 0.7}
      vectorEffect="non-scaling-stroke"
    />
  ));
}

/// The value labels left of the plot and the time labels under it, positioned
/// against the plot's own box.
function AxisLabels({ max }: { max: number }) {
  return (
    <>
      {TICKS.map((tick) => (
        <span
          key={tick}
          className={cn(AXIS_LABEL, "right-full -translate-y-1/2 pr-1.5 tabular-nums")}
          style={{ top: `${plotY(max * tick, max)}%` }}
        >
          {compactTokens(max * tick)}
        </span>
      ))}
      {TIME_LABELS.map(({ text, position }) => (
        <span key={text} className={cn(AXIS_LABEL, "top-full pt-1", position)}>
          {text}
        </span>
      ))}
    </>
  );
}

function StageLegend({ stages, colors }: { stages: StageFlow[]; colors: Map<string, string> }) {
  return (
    <ul className="flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
      {stages.map((stage) => (
        <li key={stage.stage_id} className="flex items-center gap-1.5">
          <span
            aria-hidden="true"
            className="size-2 rounded-sm"
            style={{ background: colors.get(stage.stage_id) }}
          />
          <span className="font-mono">{stage.stage_id}</span>
        </li>
      ))}
    </ul>
  );
}

function Mix({ flow }: { flow: TokenFlow }) {
  const rate = flow.plan.rate;
  const sum = MIX.reduce((total, { key }) => total + rate[key], 0);
  return (
    <div className="flex flex-col gap-2">
      <div
        role="img"
        aria-label="token mix per minute"
        className="flex h-2 overflow-hidden rounded-full bg-muted"
      >
        {MIX.map(({ key, color }) => (
          <span
            key={key}
            style={{ width: `${sum === 0 ? 0 : (rate[key] / sum) * 100}%`, background: color }}
          />
        ))}
      </div>
      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-xs">
        {MIX.map(({ key, label, color, hint }) => (
          <div key={key} className="contents">
            <dt className="flex items-center gap-1.5 text-muted-foreground" title={hint}>
              <span
                aria-hidden="true"
                className="size-2 rounded-sm"
                style={{ background: color }}
              />
              {label}
              {hint && <InfoIcon aria-hidden="true" className="size-3" />}
            </dt>
            <dd className="text-right font-mono tabular-nums">{compactTokens(rate[key])}/m</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}
