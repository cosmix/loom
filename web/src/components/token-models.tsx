import type { TokenFlow } from "@/api/schema";
import { ModelHistoryChart } from "@/components/model-history-chart";
import { compactTokens, modelGroups, modelRates, modelShares, wholePercents } from "@/lib/tokens";

const MODEL_COLORS = [
  "oklch(0.66 0.18 255)", // blue
  "oklch(0.76 0.16 75)", // amber
  "oklch(0.65 0.20 330)", // magenta
  "oklch(0.70 0.15 155)", // green
  "oklch(0.65 0.19 25)", // coral
  "oklch(0.68 0.16 295)", // violet
  "oklch(0.78 0.15 110)", // lime
  "oklch(0.72 0.12 205)", // cyan
] as const;

/// Contrast comes first; name order keeps colors stable as shares change.
function modelColors(shares: ReturnType<typeof modelShares>): Map<string, string> {
  const names = shares.map(({ model }) => model).sort();
  return new Map(
    names.map((model, index) => [
      model,
      MODEL_COLORS[index] ?? `oklch(0.58 0.13 ${(index * 137.508) % 360})`,
    ]),
  );
}

/// Radius of a circle whose circumference is 100, so a dash length is a percentage.
const RADIUS = 15.91549;
const HEADERS = [
  { text: "Model", numeric: false },
  { text: "Effort", numeric: false },
  { text: "Fresh", numeric: true },
  { text: "Cache read", numeric: true },
] as const;

/// The plan's token totals by model: a pie of fresh tokens per model, a legend
/// with each share and current rate, and a table per model and effort. Renders
/// nothing while no model has fresh tokens.
export function TokenModels({ flow }: { flow: TokenFlow }) {
  const shares = modelShares(flow.models);
  const colors = modelColors(shares);
  if (shares.length === 0) return null;
  const percents = wholePercents(shares.map(({ share }) => share));
  const rates = new Map(modelRates(flow).map(({ model, perMinute }) => [model, perMinute]));
  return (
    <div className="flex flex-col gap-3 border-t border-hairline pt-3">
      <div className="flex items-baseline justify-between gap-2">
        <h3 className="eyebrow">By model</h3>
        <span className="text-xs text-muted-foreground">fresh so far</span>
      </div>
      <ModelHistoryChart flow={flow} colors={colors} />
      <div className="@container flex flex-wrap items-start gap-4">
        <div className="flex min-w-0 basis-[26rem] flex-col items-stretch gap-4 @min-[26rem]:flex-row @min-[26rem]:items-center">
          <Pie shares={shares} colors={colors} />
          <ul
            aria-label="models"
            className="grid min-w-0 flex-1 grid-cols-[0.5rem_minmax(0,1fr)_max-content_max-content] gap-x-2 gap-y-1 text-xs"
          >
            {shares.map(({ model }, index) => (
              <li key={model} className="col-span-4 grid grid-cols-subgrid items-center">
                <span
                  aria-hidden="true"
                  className="size-2 shrink-0 rounded-sm"
                  style={{ background: colors.get(model) }}
                />
                <span className="truncate font-mono" title={model}>
                  {model}
                </span>
                <span className="text-right font-mono whitespace-nowrap tabular-nums">
                  {percents[index]}%
                </span>
                <span className="text-right font-mono whitespace-nowrap text-muted-foreground tabular-nums">
                  {compactTokens(rates.get(model) ?? 0)}/m
                </span>
              </li>
            ))}
          </ul>
        </div>
        <ModelTable flow={flow} />
      </div>
    </div>
  );
}

/// A donut of ring slices, one per model, starting at twelve o'clock.
function Pie({
  shares,
  colors,
}: {
  shares: ReturnType<typeof modelShares>;
  colors: Map<string, string>;
}) {
  let before = 0;
  return (
    <svg
      role="img"
      aria-label="fresh tokens by model"
      viewBox="0 0 42 42"
      width={112}
      height={112}
      className="shrink-0 self-center"
    >
      <circle cx={21} cy={21} r={RADIUS} fill="none" stroke="var(--muted)" strokeWidth={6} />
      {shares.map(({ model, share }) => {
        const pct = share * 100;
        const gap = shares.length > 1 ? Math.min(0.8, pct / 3) : 0;
        const offset = 25 - before - gap / 2;
        before += pct;
        return (
          <circle
            key={model}
            data-model={model}
            cx={21}
            cy={21}
            r={RADIUS}
            fill="none"
            stroke={colors.get(model)}
            strokeWidth={6}
            strokeDasharray={`${pct - gap} ${100 - pct + gap}`}
            strokeDashoffset={offset}
          />
        );
      })}
    </svg>
  );
}

function ModelTable({ flow }: { flow: TokenFlow }) {
  return (
    <table
      aria-label="token totals by model and effort"
      className="w-full min-w-[16rem] flex-1 text-xs"
    >
      <thead>
        <tr className="border-b border-hairline text-muted-foreground">
          {HEADERS.map(({ text, numeric }) => (
            <th
              key={text}
              scope="col"
              className={numeric ? "py-1 text-right font-medium" : "py-1 text-left font-medium"}
            >
              {text}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {modelGroups(flow.models).flatMap(({ model, efforts }) =>
          efforts.map((entry, row) => (
            <tr
              key={`${model}/${entry.effort ?? ""}`}
              className="border-b border-hairline last:border-b-0"
            >
              {row === 0 && (
                <td rowSpan={efforts.length} className="py-1 pr-2 align-top font-mono">
                  {model}
                </td>
              )}
              <td className="py-1 pr-2">{entry.effort ?? "—"}</td>
              <td className="py-1 text-right font-mono tabular-nums">
                {compactTokens(entry.fresh)}
              </td>
              <td className="py-1 text-right font-mono tabular-nums">
                {compactTokens(entry.cacheRead)}
              </td>
            </tr>
          )),
        )}
      </tbody>
    </table>
  );
}
