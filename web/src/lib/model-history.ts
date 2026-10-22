import type { TokenFlow } from "@/api/schema";
import { lastValues, rollingRate } from "@/lib/tokens";

const BARS = 60;
const DEFAULT_BUCKET_SECS = 30;
/// Buckets behind each bar, the backend's `RATE_BUCKETS`, so the newest bar stacks
/// to the headline fresh / min.
const RATE_BUCKETS = 10;

export interface ModelBar {
  at: number;
  rates: Record<string, number>;
}

/// The by-model bars, oldest first, from the same per-bucket agent series as the
/// stage chart. Each bar is the fresh rate per minute over the `RATE_BUCKETS`
/// buckets ending at it, the headline's five-minute window: a single 30 s bucket
/// times two would draw one subagent's opening cache write as a spike several
/// times the headline. The newest window ends at `observed_at`, as the backend's
/// `rate` does, so the newest bar equals the headline and legend rates. An agent
/// that switched models books its whole series under its current model (known
/// limitation; revisit if the backend publishes per-model series).
export function modelHistoryBars(flow: TokenFlow): ModelBar[] {
  const bucketSecs = flow.bucket_secs > 0 ? flow.bucket_secs : DEFAULT_BUCKET_SECS;
  const newest = Math.floor(flow.observed_at / bucketSecs);
  const elapsed = flow.observed_at - newest * bucketSecs;
  const filling = (RATE_BUCKETS * bucketSecs) / ((RATE_BUCKETS - 1) * bucketSecs + elapsed + 1);
  const bars: ModelBar[] = Array.from({ length: BARS }, (_, index) => ({
    at: (newest - (BARS - 1) + index) * bucketSecs,
    rates: Object.create(null),
  }));
  for (const stage of flow.stages) {
    for (const agent of stage.agents) {
      const model = agent.model ?? "unknown";
      const series = lastValues(agent.series, BARS + RATE_BUCKETS - 1);
      rollingRate(series, bucketSecs, RATE_BUCKETS)
        .slice(RATE_BUCKETS - 1)
        .forEach((rate, index) => {
          if (rate <= 0) return;
          const rates = bars[index]!.rates;
          rates[model] = (rates[model] ?? 0) + (index === BARS - 1 ? rate * filling : rate);
        });
    }
  }
  return bars;
}
