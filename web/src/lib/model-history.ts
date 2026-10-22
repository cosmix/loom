import type { TokenFlow } from "@/api/schema";
import { lastValues } from "@/lib/tokens";

const BARS = 60;
const DEFAULT_BUCKET_SECS = 30;

export interface ModelBar {
  at: number;
  rates: Record<string, number>;
}

/// The by-model bars, oldest first, built from the same per-bucket agent series
/// as the stage chart, so both agree. An agent that switched models books its
/// whole series under its current model (known limitation; revisit if the
/// backend publishes per-model series).
export function modelHistoryBars(flow: TokenFlow): ModelBar[] {
  const bucketSecs = flow.bucket_secs > 0 ? flow.bucket_secs : DEFAULT_BUCKET_SECS;
  const newest = Math.floor(flow.observed_at / bucketSecs);
  const bars: ModelBar[] = Array.from({ length: BARS }, (_, index) => ({
    at: (newest - (BARS - 1) + index) * bucketSecs,
    rates: Object.create(null),
  }));
  for (const stage of flow.stages) {
    for (const agent of stage.agents) {
      const model = agent.model ?? "unknown";
      lastValues(agent.series, BARS).forEach((value, index) => {
        if (value <= 0) return;
        const rates = bars[index]!.rates;
        rates[model] = (rates[model] ?? 0) + (value * 60) / bucketSecs;
      });
    }
  }
  return bars;
}
