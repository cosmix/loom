import type { TokenFlow } from "@/api/schema";
import { compactTokens, modelGroups, modelRates, modelShares } from "@/lib/tokens";

/// Model colours: the state tones, cycled in pie order.
const MODEL_PALETTE = [
  "var(--tone-completed)",
  "var(--tone-contract)",
  "var(--tone-warning)",
  "var(--tone-merged)",
  "var(--tone-queued)",
  "var(--tone-executing)",
] as const;

/// Radius of a circle whose circumference is 100, so a dash length is a percentage.
const RADIUS = 15.91549;
const HEADERS = [
  { text: "Model", numeric: false },
  { text: "Effort", numeric: false },
  { text: "Fresh", numeric: true },
  { text: "Cache read", numeric: true },
] as const;

const colorAt = (index: number) => MODEL_PALETTE[index % MODEL_PALETTE.length];

/// The plan's token totals by model: a pie of fresh tokens per model, a legend
/// with each share and current rate, and a table per model and effort. Renders
/// nothing while no model has fresh tokens.
export function TokenModels({ flow }: { flow: TokenFlow }) {
  const shares = modelShares(flow.models);
  if (shares.length === 0) return null;
  const rates = new Map(modelRates(flow).map(({ model, perMinute }) => [model, perMinute]));
  return (
    <div className="flex flex-col gap-3 border-t border-hairline pt-3">
      <div className="flex items-baseline justify-between gap-2">
        <h3 className="eyebrow">By model</h3>
        <span className="text-xs text-muted-foreground">fresh so far</span>
      </div>
      <div className="flex flex-wrap items-start gap-4">
        <div className="flex min-w-[14rem] items-center gap-4">
          <Pie shares={shares} />
          <ul aria-label="models" className="flex min-w-0 flex-col gap-1 text-xs">
            {shares.map(({ model, share }, index) => (
              <li key={model} className="flex items-center gap-1.5">
                <span
                  aria-hidden="true"
                  className="size-2 shrink-0 rounded-sm"
                  style={{ background: colorAt(index) }}
                />
                <span className="truncate font-mono">{model}</span>
                <span className="font-mono tabular-nums">{Math.round(share * 100)}%</span>
                <span className="font-mono text-muted-foreground tabular-nums">
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
function Pie({ shares }: { shares: ReturnType<typeof modelShares> }) {
  let before = 0;
  return (
    <svg role="img" aria-label="fresh tokens by model" viewBox="0 0 42 42" width={112} height={112}>
      <circle cx={21} cy={21} r={RADIUS} fill="none" stroke="var(--muted)" strokeWidth={6} />
      {shares.map(({ model, share }, index) => {
        const pct = share * 100;
        const offset = 25 - before;
        before += pct;
        return (
          <circle
            key={model}
            data-model={model}
            cx={21}
            cy={21}
            r={RADIUS}
            fill="none"
            stroke={colorAt(index)}
            strokeWidth={6}
            strokeDasharray={`${pct} ${100 - pct}`}
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
