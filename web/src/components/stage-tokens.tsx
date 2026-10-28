import { useAtomValue } from "jotai/react";
import { Coins } from "lucide-react";
import { useId } from "react";

import type { StageFlow, TokenFlow } from "@/api/schema";
import { HeatStrip, Sparkline } from "@/components/sparkline";
import { TokenLanes } from "@/components/stage-token-lanes";
import {
  cacheHitPercent,
  carryForward,
  codexPairs,
  compactTokens,
  cumulativeSeries,
  fresh,
  isCodexLane,
  lastValues,
  liveCounts,
  maxOf,
  rollingRate,
  stageFlow,
} from "@/lib/tokens";
import { DisclosureHeading, useDisclosure } from "@/components/use-disclosure";
import { snapshotAtom } from "@/state/atoms";

const RAMP = [0, 1, 2, 3, 4];
/// The chips' trends show the last 60 buckets: 30 minutes of 30 s cells.
const TREND_POINTS = 60;

interface Trend {
  values: number[];
  label: string;
}

/// Each chip's trend over the last `TREND_POINTS` buckets of the stage's series.
function chipTrends(tokens: TokenFlow, flow: StageFlow) {
  return {
    rate: lastValues(rollingRate(flow.series, tokens.bucket_secs), TREND_POINTS),
    total: lastValues(cumulativeSeries(fresh(flow.total), flow.series), TREND_POINTS),
    hit: lastValues(carryForward(flow.hit_series), TREND_POINTS),
    live: lastValues(liveCounts(flow.agents, flow.series.length), TREND_POINTS),
  };
}

/// The lanes keep the whole series the API sends (120 buckets, 60 min) so an
/// agent that ran an hour ago still lights its strip. Revisit if the API's
/// bucket count changes; the cap also bounds the DOM for a runaway series.
const LANE_POINTS = 120;

/// The lanes' series over one window of cells: as long as the longest series
/// (at least `TREND_POINTS`, at most `LANE_POINTS`), so the strips keep one
/// width. Newer buckets sit on the right and a young or idle series starts with
/// empty cells on the left. Also returns the maximum the cells are scaled to,
/// over the visible window, and the window's span in minutes. Loops over the
/// series rather than spreading them into arguments, which throws a RangeError
/// on a long series.
function laneSeries(
  stage: StageFlow,
  bucketSecs: number,
): { lanes: number[][]; max: number; minutes: number } {
  const longest = stage.agents.reduce(
    (best, agent) => Math.max(best, agent.series.length),
    stage.series.length,
  );
  const cells = Math.min(LANE_POINTS, Math.max(TREND_POINTS, longest));
  const lanes = stage.agents.map((agent) => lastValues(agent.series, cells));
  const max = lanes.reduce(
    (best, series) => maxOf(series, best),
    maxOf(lastValues(stage.series, cells)),
  );
  return { lanes, max, minutes: Math.round((cells * bucketSecs) / 60) };
}

function Chip({ value, label, trend }: { value: string; label: string; trend: Trend }) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5 rounded-md border border-hairline bg-background px-2.5 py-1.5">
      <div className="flex flex-wrap items-baseline justify-between gap-x-2">
        <span className="font-mono text-sm tabular-nums">{value}</span>
        <span className="ml-auto text-right text-[11px] text-muted-foreground">{label}</span>
      </div>
      <Sparkline
        values={trend.values}
        width={96}
        height={20}
        stretch
        label={trend.label}
        className="text-(--tone-executing)"
      />
    </div>
  );
}

/// The four headline chips, each with the trend of its own statistic. They
/// render the same whether the section is folded or open.
function Chips({ tokens, flow }: { tokens: TokenFlow; flow: StageFlow }) {
  const hit = cacheHitPercent(flow.total);
  const live = flow.agents.filter((agent) => agent.active).length;
  const trends = chipTrends(tokens, flow);
  return (
    <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
      <Chip
        value={`${compactTokens(fresh(flow.rate))}/m`}
        label="fresh, 5 min avg"
        trend={{ values: trends.rate, label: "fresh rate, last 30 min" }}
      />
      <Chip
        value={compactTokens(fresh(flow.total))}
        label="fresh so far"
        trend={{ values: trends.total, label: "fresh so far, last 30 min" }}
      />
      <Chip
        value={hit === null ? "—" : `${hit.toFixed(1)}%`}
        label="cache hit"
        trend={{ values: trends.hit, label: "cache hit per 30 s, last 30 min" }}
      />
      <Chip
        value={`${live} of ${flow.agents.length}`}
        label="agents live"
        trend={{ values: trends.live, label: "agents live, last 30 min" }}
      />
    </div>
  );
}

/// The stage's token flow: headline chips and one heat-strip lane per agent
/// active in the window, each forwarder and its codex job clustered last, the
/// idle ones folded under "earlier agents". Folded to the chips' labels
/// and trends until the reader opens it.
/// Renders nothing for a stage the tracker has not seen.
export function StageTokens({ stageId }: { stageId: string }) {
  const tokens = useAtomValue(snapshotAtom)?.status.tokens;
  const flow = stageFlow(tokens, stageId);
  const [open, toggle] = useDisclosure(false);
  const panel = useId();
  if (!tokens || !flow) return null;

  const { lanes, max, minutes } = laneSeries(flow, tokens.bucket_secs);
  const rows = flow.agents.map((agent, index) => ({
    agent,
    series: lanes[index] ?? [],
  }));
  const plain = rows.filter(({ agent }) => !isCodexLane(agent));
  const clusters = codexPairs(rows);

  return (
    <section aria-label="stage tokens" className="ledger-card">
      <header className="ledger-head">
        <DisclosureHeading
          label="tokens"
          open={open}
          panel={panel}
          onToggle={toggle}
          icon={
            <span className="ledger-icon">
              <Coins aria-hidden="true" className="size-3" />
            </span>
          }
        />
        {open && (
          <div className="ml-auto flex items-center gap-2 text-[11px] text-muted-foreground">
            <span>{tokens.bucket_secs} s cells</span>
            <span className="font-mono tabular-nums">0</span>
            <HeatStrip
              values={RAMP}
              max={4}
              label={`intensity, 0 to ${max} fresh tokens per ${tokens.bucket_secs} s`}
              className="w-16"
            />
            <span className="font-mono tabular-nums">{compactTokens(max)}</span>
          </div>
        )}
      </header>
      <div id={open ? panel : undefined} className="flex flex-col gap-2.5 p-3.5">
        <Chips tokens={tokens} flow={flow} />
        {open && (
          <TokenLanes plain={plain} clusters={clusters} max={max} window={`last ${minutes} min`} />
        )}
      </div>
    </section>
  );
}
