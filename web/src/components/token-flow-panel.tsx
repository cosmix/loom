import { cn } from "cn";
import { useAtomValue } from "jotai/react";

import type { StageFlow, TokenFlow, TokenUsage } from "@/api/schema";
import { cacheHitPercent, compactTokens, fresh, lastValues, maxOf, modelRates } from "@/lib/tokens";
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

/// Stage colours: the existing state tones, cycled in `stages` order.
const PALETTE = [
  "var(--tone-executing)",
  "var(--tone-queued)",
  "var(--tone-completed)",
  "var(--tone-warning)",
  "var(--tone-contract)",
  "var(--tone-merged)",
] as const;

const MIX = [
  { key: "input", label: "uncached input", color: "var(--tone-executing)" },
  { key: "cache_write", label: "cache write", color: "var(--tone-warning)" },
  { key: "output", label: "output", color: "var(--tone-completed)" },
  { key: "cache_read", label: "cache read", color: "var(--tone-dimmed)" },
] as const satisfies ReadonlyArray<{ key: keyof TokenUsage; label: string; color: string }>;

/// The plan's token flow: the fresh rate, a stacked chart of each stage's last
/// 30 minutes, the token mix and the per-model rates. Renders nothing while
/// the snapshot carries no token flow.
export function TokenFlowPanel() {
  const flow = useAtomValue(snapshotAtom)?.status.tokens;
  if (!flow) return null;
  return (
    <section aria-label="Token flow" className="flex min-w-0 flex-col gap-3">
      <div className="flex items-baseline justify-between gap-2">
        <h2 className="eyebrow">Token flow</h2>
        <span className="text-xs text-muted-foreground">5 min avg</span>
      </div>
      <div className="flex flex-col gap-4 rounded-lg border border-hairline bg-card/60 p-4 shadow-xs">
        <div className="flex items-baseline gap-2">
          <span className="font-mono text-3xl leading-none font-semibold tabular-nums">
            {compactTokens(fresh(flow.plan.rate))}
          </span>
          <span className="text-sm text-muted-foreground">fresh / min</span>
        </div>
        <StageChart stages={flow.stages} />
        <Mix flow={flow} />
        <Models flow={flow} />
      </div>
    </section>
  );
}

/// Where a chart value sits in the 100 x 100 plot box, as a percentage.
const plotX = (index: number) => (index / (POINTS - 1)) * 100;
const plotY = (value: number, max: number) => 100 - (value / max) * 100;

/// One stage's polygon: its `id` and the svg `points` outlining its layer.
interface Band {
  id: string;
  points: string;
}

/// One polygon per stage, stacked in `stages` order, and the chart's top: the tallest
/// stacked cell, at least 1.
function stackBands(stages: StageFlow[]): { bands: Band[]; max: number } {
  const base = Array.from({ length: POINTS }, () => 0);
  const layers = stages.map((stage) => {
    const lower = [...base];
    lastValues(stage.series, POINTS).forEach((value, index) => {
      base[index] = (base[index] ?? 0) + value;
    });
    return { id: stage.stage_id, lower, upper: [...base] };
  });
  const max = maxOf(base, 1);
  const point = (value: number, index: number) =>
    `${plotX(index).toFixed(2)},${plotY(value, max).toFixed(2)}`;
  const bands = layers.map(({ id, lower, upper }) => ({
    id,
    points: [...upper.map(point), ...lower.map(point).reverse()].join(" "),
  }));
  return { bands, max };
}

function StageChart({ stages }: { stages: StageFlow[] }) {
  const { bands, max } = stackBands(stages);
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
          <BandPlot bands={bands} max={max} />
          <AxisLabels max={max} />
        </div>
      </div>
      <StageLegend stages={stages} />
    </div>
  );
}

function BandPlot({ bands, max }: { bands: Band[]; max: number }) {
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
      {TICKS.map((tick) => (
        <line
          key={tick}
          x1={0}
          x2={100}
          y1={plotY(max * tick, max)}
          y2={plotY(max * tick, max)}
          stroke="var(--hairline)"
          strokeWidth={1}
          vectorEffect="non-scaling-stroke"
        />
      ))}
      {bands.map((band, index) => (
        <polygon
          key={band.id}
          points={band.points}
          fill={PALETTE[index % PALETTE.length]}
          fillOpacity={0.55}
          stroke={PALETTE[index % PALETTE.length]}
          strokeWidth={1}
          vectorEffect="non-scaling-stroke"
        />
      ))}
    </svg>
  );
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

function StageLegend({ stages }: { stages: StageFlow[] }) {
  return (
    <ul className="flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
      {stages.map((stage, index) => (
        <li key={stage.stage_id} className="flex items-center gap-1.5">
          <span
            aria-hidden="true"
            className="size-2 rounded-sm"
            style={{ background: PALETTE[index % PALETTE.length] }}
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
  const hit = cacheHitPercent(rate);
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
        {MIX.map(({ key, label, color }) => (
          <div key={key} className="contents">
            <dt className="flex items-center gap-1.5 text-muted-foreground">
              <span
                aria-hidden="true"
                className="size-2 rounded-sm"
                style={{ background: color }}
              />
              {label}
            </dt>
            <dd className="text-right font-mono tabular-nums">{compactTokens(rate[key])}/m</dd>
          </div>
        ))}
        <dt className="text-muted-foreground">cache hit (5 min)</dt>
        <dd className="text-right font-mono tabular-nums">
          {hit === null ? "—" : `${Math.round(hit)}%`}
        </dd>
      </dl>
    </div>
  );
}

function Models({ flow }: { flow: TokenFlow }) {
  const rates = modelRates(flow);
  if (rates.length === 0) return null;
  return (
    <dl className="grid grid-cols-[1fr_auto] gap-x-4 gap-y-1 border-t border-hairline pt-3 text-xs">
      {rates.map(({ model, perMinute }) => (
        <div key={model} className="contents">
          <dt className="truncate font-mono text-muted-foreground">{model}</dt>
          <dd className="text-right font-mono tabular-nums">{compactTokens(perMinute)}/m</dd>
        </div>
      ))}
    </dl>
  );
}
