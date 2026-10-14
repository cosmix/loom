import type { AgentFlow, QuotaLeft, StageFlow, TokenFlow, TokenUsage } from "@/api/schema";
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
