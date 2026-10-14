import { useAtomValue } from "jotai/react";

import type { StageFlow, TokenFlow, TokenUsage } from "@/api/schema";
import { cacheHitPercent, compactTokens, fresh, modelRates } from "@/lib/tokens";
import { snapshotAtom } from "@/state/atoms";

const CHART_WIDTH = 480;
const CHART_HEIGHT = 120;
const LEFT = 34;
const TOP = 8;
const BOTTOM = 18;
const POINTS = 60;
const TICKS = [0, 0.5, 1] as const;

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
  { key: "input", label: "fresh input", color: "var(--tone-executing)" },
  { key: "cache_write", label: "cache write", color: "var(--tone-warning)" },
  { key: "output", label: "output", color: "var(--tone-completed)" },
  { key: "cache_read", label: "cache read", color: "var(--tone-dimmed)" },
] as const satisfies ReadonlyArray<{ key: keyof TokenUsage; label: string; color: string }>;

/// The last `POINTS` values of a series, left-padded with zeros: an idle
/// agent's or a short series carries fewer values than the chart draws.
function lastPoints(series: readonly number[]): number[] {
  const tail = series.slice(-POINTS);
  return [...Array<number>(POINTS - tail.length).fill(0), ...tail];
}

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

function StageChart({ stages }: { stages: StageFlow[] }) {
  const series = stages.map((stage) => lastPoints(stage.series));
  const totals = Array.from({ length: POINTS }, (_, index) =>
    series.reduce((sum, values) => sum + (values[index] ?? 0), 0),
  );
  const max = Math.max(1, ...totals);
  const plotWidth = CHART_WIDTH - LEFT;
  const plotHeight = CHART_HEIGHT - TOP - BOTTOM;
  const x = (index: number) => LEFT + (index / (POINTS - 1)) * plotWidth;
  const y = (value: number) => TOP + plotHeight - (value / max) * plotHeight;

  const base = Array.from({ length: POINTS }, () => 0);
  const bands = series.map((values, stage) => {
    const lower = [...base];
    values.forEach((value, index) => {
      base[index] = (base[index] ?? 0) + value;
    });
    const top = base.map((value, index) => `${x(index)},${y(value)}`);
    const bottom = lower.map((value, index) => `${x(index)},${y(value)}`).reverse();
    return { id: stages[stage]?.stage_id ?? "", points: [...top, ...bottom].join(" ") };
  });

  return (
    <div className="flex flex-col gap-2">
      <svg
        role="img"
        aria-label="fresh tokens per stage, last 30 min"
        viewBox={`0 0 ${CHART_WIDTH} ${CHART_HEIGHT}`}
        className="h-auto w-full"
      >
        {TICKS.map((tick) => (
          <g key={tick}>
            <line
              x1={LEFT}
              x2={CHART_WIDTH}
              y1={y(max * tick)}
              y2={y(max * tick)}
              stroke="var(--hairline)"
              strokeWidth={1}
            />
            <text
              x={LEFT - 4}
              y={y(max * tick) + 3}
              textAnchor="end"
              fontSize={9}
              fill="var(--muted-foreground)"
            >
              {compactTokens(max * tick)}
            </text>
          </g>
        ))}
        {bands.map((band, index) => (
          <polygon
            key={band.id}
            points={band.points}
            fill={PALETTE[index % PALETTE.length]}
            fillOpacity={0.55}
            stroke={PALETTE[index % PALETTE.length]}
            strokeWidth={1}
          />
        ))}
        <text x={LEFT} y={CHART_HEIGHT - 4} fontSize={9} fill="var(--muted-foreground)">
          −30m
        </text>
        <text
          x={LEFT + plotWidth / 2}
          y={CHART_HEIGHT - 4}
          textAnchor="middle"
          fontSize={9}
          fill="var(--muted-foreground)"
        >
          −15m
        </text>
        <text
          x={CHART_WIDTH}
          y={CHART_HEIGHT - 4}
          textAnchor="end"
          fontSize={9}
          fill="var(--muted-foreground)"
        >
          now
        </text>
      </svg>
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
    </div>
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
        <dt className="text-muted-foreground">cache hit</dt>
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
