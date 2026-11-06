import { IMMUTABLE_STAGE_KEYS, type StateFrame, type TickFrame } from "@/api/frames";
import type { Attention, Snapshot, StageSummary, TokenFlow } from "@/api/schema";

// How the `/ws` delta messages (loom/src/commands/status/web/feed.rs) change a
// snapshot. All pure: the previous snapshot is never mutated, and stages or
// attention entries a message does not touch keep their object identity, so
// selectors that compare by reference stay quiet.

type ImmutablePart = Pick<StageSummary, (typeof IMMUTABLE_STAGE_KEYS)[number]>;
type Clock = TickFrame["stages"][string];

const CLOCK_FIELDS = [
  ["elapsed", "elapsed_secs"],
  ["exec", "execution_secs"],
  ["stale", "staleness_secs"],
  ["judge", "judge_heartbeat_secs"],
] as const;

function immutablePart(stage: StageSummary): ImmutablePart {
  return Object.fromEntries(IMMUTABLE_STAGE_KEYS.map((key) => [key, stage[key]])) as ImmutablePart;
}

function clockFor(listed: TickFrame["stages"], id: string): Clock | undefined {
  return Object.hasOwn(listed, id) ? listed[id] : undefined;
}

/** Apply a `state` message: changed stages are replaced whole, keeping their immutable keys. */
export function mergeState(prev: Snapshot, frame: StateFrame): Snapshot {
  let status = frame.status ? { ...prev.status, ...frame.status } : prev.status;
  if (frame.stages) {
    status = { ...status, stages: replaceStages(prev.status.stages, frame.stages) };
  }
  const next: Snapshot = { ...prev, status, generated_at: frame.at };
  if (frame.attention) next.attention = frame.attention;
  if (frame.alerts) next.alerts = frame.alerts;
  if (frame.daemon !== undefined) next.daemon = frame.daemon;
  if (frame.source !== undefined) next.source = frame.source;
  if (frame.notice === null) delete next.notice;
  else if (frame.notice !== undefined) next.notice = frame.notice;
  return next;
}

function replaceStages(
  stages: StageSummary[],
  changed: NonNullable<StateFrame["stages"]>,
): StageSummary[] {
  const known = new Set(stages.map((stage) => stage.id));
  for (const id of Object.keys(changed)) {
    if (!known.has(id)) console.error("dashboard: state for an unknown stage", id);
  }
  return stages.map((stage) => {
    // Replaced whole: a key the server stopped sending (an optional one) must go.
    const dynamic = Object.hasOwn(changed, stage.id) ? changed[stage.id] : undefined;
    return dynamic ? ({ ...immutablePart(stage), ...dynamic } as StageSummary) : stage;
  });
}

/** Apply a `tick` message: the clock values and the frame's stamp. */
export function mergeTick(prev: Snapshot, frame: TickFrame): Snapshot {
  const moved = Object.keys(frame.stages).length > 0;
  const tickStages = () =>
    prev.status.stages.map((stage) => tickStage(stage, clockFor(frame.stages, stage.id)));
  const status = moved ? { ...prev.status, stages: tickStages() } : prev.status;
  const attention = moved
    ? prev.attention.map((entry) => tickAttention(entry, clockFor(frame.stages, entry.id)))
    : prev.attention;
  return { ...prev, status, attention, generated_at: frame.at, tick_age_secs: frame.tick_age };
}

function tickStage(stage: StageSummary, clock: Clock | undefined): StageSummary {
  if (!clock) return stage;
  const next = { ...stage };
  for (const [name, field] of CLOCK_FIELDS) {
    const value = clock[name];
    if (value !== undefined) next[field] = value;
  }
  // A freeze job's progress is a state change; only its clock arrives here.
  if (clock.contract !== undefined && stage.contract_testing) {
    next.contract_testing = { ...stage.contract_testing, running_secs: clock.contract };
  }
  return next;
}

// An attention entry mirrors its stage's judge heartbeat while one is running.
function tickAttention(entry: Attention, clock: Clock | undefined): Attention {
  if (clock?.judge === undefined || entry.judge_heartbeat_secs === null) return entry;
  return { ...entry, judge_heartbeat_secs: clock.judge };
}

/** Apply a `tokens` message: replace the token flow, or drop it. */
export function mergeTokens(prev: Snapshot, tokens: TokenFlow | undefined): Snapshot {
  const status = { ...prev.status };
  if (tokens === undefined) delete status.tokens;
  else status.tokens = tokens;
  return { ...prev, status };
}
