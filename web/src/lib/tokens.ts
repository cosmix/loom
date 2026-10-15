import type {
  AgentFlow,
  ModelUsage,
  QuotaLeft,
  StageFlow,
  TokenFlow,
  TokenUsage,
} from "@/api/schema";
import type { Tone } from "@/lib/format";

export function fresh(u: TokenUsage): number {
  return u.input + u.cache_write + u.output;
}

/// The largest of `values`, at least `floor`. A loop, not `Math.max(...values)`:
/// spreading a long series into arguments throws a RangeError.
export function maxOf(values: readonly number[], floor = 0): number {
  let max = floor;
  for (const value of values) if (value > max) max = value;
  return max;
}

/// `series` left-padded with zeros to `length`; a longer series is returned whole.
export function padLeft(series: readonly number[], length: number): number[] {
  return [...Array<number>(Math.max(0, length - series.length)).fill(0), ...series];
}

/// The last `length` values of `series`, left-padded with zeros: an idle or
/// young series carries fewer values than a sparkline or chart draws, and
/// unpadded it would stretch across the full width.
export function lastValues(series: readonly number[], length: number): number[] {
  return padLeft(series.slice(-length), length);
}

export function compactTokens(n: number): string {
  if (n < 1000) return String(Math.floor(n));
  if (n < 100000) return `${(Math.floor(n / 100) / 10).toFixed(1)}k`;
  if (n < 1000000) return `${Math.floor(n / 1000)}k`;
  return `${(Math.floor(n / 10000) / 100).toFixed(2)}M`;
}

export function cacheHitPercent(u: TokenUsage): number | null {
  const denominator = u.cache_read + u.input + u.cache_write;
  return denominator === 0 ? null : (u.cache_read / denominator) * 100;
}

export function stageFlow(flow: TokenFlow | undefined, stageId: string): StageFlow | undefined {
  return flow?.stages.find((stage) => stage.stage_id === stageId);
}

export function agentLabel(a: AgentFlow): string {
  if (a.kind === "main") return "main session";
  if (a.kind === "codex") return "codex job";
  return a.agent_type ?? "subagent";
}

export function quotaLeftText(q: QuotaLeft): { text: string; tone: Tone } | null {
  if (q.lasts_to_reset) return { text: "lasts to reset", tone: "dimmed" };
  const minutes = q.minutes_left;
  if (minutes === null) return null;
  const duration =
    minutes >= 60
      ? `${Math.floor(minutes / 60)}h${String(minutes % 60).padStart(2, "0")}m`
      : `${minutes}m`;
  return { text: `~${duration} left`, tone: minutes < 30 ? "blocked" : "warning" };
}

export function modelRates(flow: TokenFlow): Array<{ model: string; perMinute: number }> {
  const rates = new Map<string, number>();
  for (const stage of flow.stages) {
    for (const agent of stage.agents) {
      const model = agent.model ?? "unknown";
      rates.set(model, (rates.get(model) ?? 0) + fresh(agent.rate));
    }
  }
  return Array.from(rates, ([model, perMinute]) => ({ model, perMinute })).sort(
    (a, b) => b.perMinute - a.perMinute,
  );
}

/// One model's fresh tokens over all its efforts, and its share (0 to 1) of every model's fresh.
export interface ModelShare {
  model: string;
  fresh: number;
  share: number;
}

/// One effort's fresh and cache-read totals within a model.
export interface EffortTotals {
  effort: string | null;
  fresh: number;
  cacheRead: number;
}

/// One model with its totals and its efforts in input order.
export interface ModelGroup {
  model: string;
  fresh: number;
  cacheRead: number;
  efforts: EffortTotals[];
}

/// `models` grouped by model name, ordered by fresh descending then model ascending.
/// Empty when there are no models.
export function modelGroups(models: readonly ModelUsage[]): ModelGroup[] {
  const groups = new Map<string, ModelGroup>();
  for (const { model, effort, total } of models) {
    const group = groups.get(model) ?? { model, fresh: 0, cacheRead: 0, efforts: [] };
    const used = fresh(total);
    group.fresh += used;
    group.cacheRead += total.cache_read;
    group.efforts.push({ effort, fresh: used, cacheRead: total.cache_read });
    groups.set(model, group);
  }
  return Array.from(groups.values()).sort(
    (a, b) => b.fresh - a.fresh || (a.model < b.model ? -1 : a.model > b.model ? 1 : 0),
  );
}

/// Each model's fresh tokens (efforts summed) and share of the total, in `modelGroups`
/// order. Empty when there are no models or no fresh tokens at all.
export function modelShares(models: readonly ModelUsage[]): ModelShare[] {
  const groups = modelGroups(models);
  let sum = 0;
  for (const group of groups) sum += group.fresh;
  if (sum === 0) return [];
  return groups.map(({ model, fresh: used }) => ({ model, fresh: used, share: used / sum }));
}

/// Tokens per minute over a trailing window of `window` buckets: element `i` sums
/// `series[i - window + 1 .. i]` (fewer at the start) and divides by the full window,
/// so a young series ramps up. A running sum keeps it linear.
export function rollingRate(series: readonly number[], bucketSecs: number, window = 10): number[] {
  const scale = 60 / (window * bucketSecs);
  const out: number[] = [];
  let sum = 0;
  for (let i = 0; i < series.length; i++) {
    sum += series[i];
    if (i >= window) sum -= series[i - window];
    out.push(sum * scale);
  }
  return out;
}

/// The running total behind `series`: element `i` is `total` minus the buckets after
/// it, never below 0, so the last element equals `total`.
export function cumulativeSeries(total: number, series: readonly number[]): number[] {
  const out = Array<number>(series.length).fill(0);
  let after = 0;
  for (let i = series.length - 1; i >= 0; i--) {
    out[i] = Math.max(0, total - after);
    after += series[i];
  }
  return out;
}

/// `values` with each gap (`null`) holding the previous value; leading gaps are 0.
export function carryForward(values: ReadonlyArray<number | null>): number[] {
  let last = 0;
  return values.map((value) => {
    if (value !== null) last = value;
    return last;
  });
}

/// How many agents had activity in the trailing `window` buckets, per bucket. Each
/// agent's series is left-padded to `length` first.
export function liveCounts(agents: readonly AgentFlow[], length: number, window = 4): number[] {
  const counts = Array<number>(length).fill(0);
  for (const agent of agents) {
    const series = padLeft(agent.series, length);
    let lastActive = -Infinity;
    for (let i = 0; i < length; i++) {
      if (series[i] > 0) lastActive = i;
      if (i - lastActive < window) counts[i] += 1;
    }
  }
  return counts;
}
