import type { AgentFlow, StageFlow, StageSummary } from "@/api/schema";
import { Sparkline } from "@/components/sparkline";
import { compactTokens, fresh, lastValues } from "@/lib/tokens";

/// Twenty 30 s buckets: the ten minutes the ledger row also draws.
const TRACE_POINTS = 20;
const TRACE_WIDTH = 108;
const TRACE_HEIGHT = 14;
/// Live agents past this many collapse into a "+n" so the row keeps its width.
const MAX_PIPS = 4;

/// The token row at the foot of a stage card. An executing stage draws its
/// last ten minutes as a trace, its live agents as pips, and its rate; any
/// other stage states what it spent in all.
export function NodeTokens({ stage, tokens }: { stage: StageSummary; tokens: StageFlow }) {
  if (stage.status !== "executing") {
    return (
      <div className="node-tokens" data-mode="total">
        <span className="node-tokens-label">tokens</span>
        <span className="node-figure" title="fresh tokens spent in all">
          {compactTokens(fresh(tokens.total))}
          <span className="node-figure-unit"> total</span>
        </span>
      </div>
    );
  }
  return (
    <div className="node-tokens" data-mode="rate">
      <Sparkline
        values={lastValues(tokens.series, TRACE_POINTS)}
        width={TRACE_WIDTH}
        height={TRACE_HEIGHT}
        label={`${stage.name} tokens, last 10 min`}
        className="node-trace"
      />
      <AgentPips agents={tokens.agents} />
      <span className="node-figure" title="fresh tokens per minute, 5 min average">
        {compactTokens(fresh(tokens.rate))}
        <span className="node-figure-unit">/m</span>
      </span>
    </div>
  );
}

/// One filled pip per live agent, read from `active` as sent; a single hollow
/// pip when none is live. The label carries the full count.
export function AgentPips({ agents }: { agents: readonly AgentFlow[] }) {
  if (agents.length === 0) return null;
  const live = agents.filter((agent) => agent.active).length;
  const label = `${live} of ${agents.length} agent${agents.length === 1 ? "" : "s"} live`;
  const pips = Math.max(1, Math.min(live, MAX_PIPS));
  return (
    <span role="img" aria-label={label} title={label} className="node-pips">
      {Array.from({ length: pips }, (_, index) => (
        <span key={index} className="node-pip" data-active={live > 0 || undefined} />
      ))}
      {live > MAX_PIPS && <span className="node-pips-more">+{live - MAX_PIPS}</span>}
    </span>
  );
}
