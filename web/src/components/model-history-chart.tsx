import type { TokenFlow } from "@/api/schema";
import { modelHistoryBars, type ModelBar } from "@/lib/model-history";
import { compactTokens, maxOf } from "@/lib/tokens";

const TICKS = [0, 0.5, 1] as const;
const LABEL = "absolute text-[11px] leading-none text-muted-foreground tabular-nums";
const OTHER_COLOR = "var(--tone-dimmed)";
/// Space between stacked segments, in plot units (the 112 px plot is 100 units tall).
const SEGMENT_GAP = 1.5;

export function ModelHistoryChart({
  flow,
  colors,
}: {
  flow: TokenFlow;
  colors: Map<string, string>;
}) {
  const bars = modelHistoryBars(flow);
  const peak = maxOf(
    bars.map((bar) => Object.values(bar.rates).reduce((sum, rate) => sum + rate, 0)),
    1,
  );
  const ready = bars.some((bar) => Object.keys(bar.rates).length > 0);
  return (
    <figure className="min-w-0" aria-label="Model token rate history">
      <figcaption className="flex items-baseline justify-between gap-2 text-xs text-muted-foreground">
        <span>Fresh / min</span>
        <span>Last 30 min · 30 s bars</span>
      </figcaption>
      <ModelPlot bars={bars} colors={colors} max={chartCeiling(peak)} ready={ready} />
    </figure>
  );
}

function chartCeiling(peak: number): number {
  const power = 10 ** Math.floor(Math.log10(Math.max(1, peak)));
  const step = [1, 2, 5, 10].find((value) => value * power >= peak) ?? 10;
  return step * power;
}

function ModelPlot({
  bars,
  colors,
  max,
  ready,
}: {
  bars: ModelBar[];
  colors: Map<string, string>;
  max: number;
  ready: boolean;
}) {
  return (
    <div className="relative mt-3 mb-5 ml-10">
      <svg
        role="img"
        aria-label="Fresh tokens per minute by model, stacked bars, last 30 min"
        viewBox="0 0 100 100"
        preserveAspectRatio="none"
        className="block h-28 w-full overflow-visible"
      >
        <GridLines />
        {bars.map((bar, index) => (
          <StackedBar
            key={bar.at}
            bar={bar}
            index={index}
            newest={bars[bars.length - 1]!.at}
            colors={colors}
            max={max}
          />
        ))}
      </svg>
      <AxisLabels max={max} />
      {!ready && (
        <p className="absolute inset-0 flex items-center justify-center text-center text-xs text-muted-foreground">
          No fresh tokens in the last 30 min
        </p>
      )}
    </div>
  );
}

function GridLines() {
  return TICKS.map((tick) => (
    <line
      key={tick}
      x1={0}
      x2={100}
      y1={(1 - tick) * 100}
      y2={(1 - tick) * 100}
      stroke="var(--hairline)"
      strokeWidth={1}
      vectorEffect="non-scaling-stroke"
    />
  ));
}

function AxisLabels({ max }: { max: number }) {
  return (
    <>
      {TICKS.map((tick) => (
        <span
          key={tick}
          className={`${LABEL} right-full -translate-y-1/2 pr-1.5`}
          style={{ top: `${(1 - tick) * 100}%` }}
        >
          {compactTokens(max * tick)}
        </span>
      ))}
      <span className={`${LABEL} top-full left-0 pt-1`}>−30m</span>
      <span className={`${LABEL} top-full left-1/2 -translate-x-1/2 pt-1`}>−15m</span>
      <span className={`${LABEL} top-full right-0 pt-1`}>now</span>
    </>
  );
}

function StackedBar({
  bar,
  index,
  newest,
  colors,
  max,
}: {
  bar: ModelBar;
  index: number;
  newest: number;
  colors: Map<string, string>;
  max: number;
}) {
  let below = 0;
  // Models the backend folded into `other` have no colour; stack them last, dimmed.
  const segments: [string, string][] = [
    ...[...colors].filter(([model]) => bar.rates[model] !== undefined),
    ...Object.keys(bar.rates)
      .filter((model) => !colors.has(model))
      .sort()
      .map((model): [string, string] => [model, OTHER_COLOR]),
  ];
  const total = Object.values(bar.rates).reduce((sum, rate) => sum + rate, 0);
  return (
    <g data-time={bar.at}>
      <title>{`${Math.round((newest - bar.at) / 60)} min ago · ${compactTokens(total)} fresh / min`}</title>
      {segments.map(([model, color], position) => {
        const rate = bar.rates[model]!;
        const span = (rate / max) * 100;
        below += span;
        // Every segment above the first gives up its bottom edge to the gap,
        // so the bar's top still sits at its total.
        const height = Math.max(0, span - (position === 0 ? 0 : SEGMENT_GAP));
        return (
          <rect
            key={model}
            data-model={model}
            data-rate={rate}
            x={((index + 0.1) * 100) / 60}
            y={100 - below}
            width={(0.8 * 100) / 60}
            height={height}
            fill={color}
          >
            <title>
              {model} · {compactTokens(rate)} fresh / min
            </title>
          </rect>
        );
      })}
    </g>
  );
}
