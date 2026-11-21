import type { PlanPhase, PlanStageStats, PlanStats } from "@/api/schema";
import { modelWithEffort } from "@/lib/format";
import { maxOf } from "@/lib/tokens";

import { clockHm, formatSpan } from "./plan-math";

export const PHASE_LABEL: Record<PlanPhase, string> = {
  contract: "Contract writing",
  executing: "Executing",
  adjudication: "Dispute and adjudication",
  retry: "Retry",
  waiting: "Waiting or merging",
};

export const WIDTH = 960;
export const LEFT = 210;
export const RIGHT = 70;
export const TOP = 34;
export const ROW = 38;
export const BAR_TOP = 8;
export const BAR_HEIGHT = 20;
const BOTTOM = 18;
/// Past this many rows the hour labels are repeated under the last row, where the top axis has
/// scrolled out of view.
const REPEAT_AXIS_ROWS = 12;
const AXIS_REPEAT = 16;
/// A phase shorter than this many units would vanish, so it is drawn this wide.
const MIN_SEGMENT = 2;
/// What fits the label column (`LEFT` minus its inset) in the 12-unit mono face.
const LABEL_CHARS = 26;
const SUB_CHARS = 32;
/// The bars share one scale: `LEFT` to `WIDTH - RIGHT` spans the plan's start to its end.
const SPAN = WIDTH - LEFT - RIGHT;
const MINUTE_MS = 60_000;
/// Tick spacings in minutes, smallest first; the layout takes the first one that fits MAX_TICKS.
const TICK_STEPS = [1, 5, 15, 30, 60, 120, 360, 720, 1440];
const MAX_TICKS = 8;
/// A tick closer than this to the end marker is dropped so their labels do not overlap.
const END_CLEARANCE = 40;

export interface Segment {
  key: string;
  phase: PlanPhase;
  x: number;
  width: number;
  title: string;
}

export interface TimelineRow {
  id: string;
  /// The id cut to the label column; the row's `<title>` carries the whole id.
  label: string;
  sub: string;
  y: number;
  skipped: boolean;
  barX: number;
  barWidth: number;
  segments: Segment[];
  duration: string;
  durationX: number;
}

export interface Tick {
  x: number;
  label: string;
}

export interface TimelineLayout {
  height: number;
  /// Where the grid lines stop, under the last row.
  gridBottom: number;
  /// Baseline of the repeated hour labels, null on a timeline short enough to need none.
  axisRepeatY: number | null;
  rows: TimelineRow[];
  ticks: Tick[];
  end: Tick;
}

interface Scale {
  startMs: number;
  totalMs: number;
  x: (ms: number) => number;
}

function scaleOf(record: PlanStats): Scale {
  const startMs = Date.parse(record.started_at);
  const totalMs = Math.max(Date.parse(record.ended_at) - startMs, 1);
  const x = (ms: number) => {
    const ratio = Number.isFinite(ms) ? (ms - startMs) / totalMs : 0;
    return LEFT + Math.min(1, Math.max(0, ratio)) * SPAN;
  };
  return { startMs, totalMs, x };
}

function clip(text: string, max: number): string {
  return text.length <= max ? text : `${text.slice(0, max - 1)}…`;
}

/// Only a skipped stage draws as a stub; a stage that recorded neither a start nor a phase
/// draws an empty bar, as the terminal screen does.
function isSkipped(stage: PlanStageStats): boolean {
  return stage.status === "skipped";
}

/// The latest `ended_at` among the phases, NaN when none has ended: the terminal screen's
/// fallback end of a stage.
function lastPhaseEnd(stage: PlanStageStats): number {
  const ends = stage.phases.flatMap((span) => (span.ended_at ? [Date.parse(span.ended_at)] : []));
  return ends.length > 0 ? maxOf(ends, Number.NEGATIVE_INFINITY) : Number.NaN;
}

/// A stage with neither a start nor a phase has no extent: no bar, the recorded execution time.
function untimedRow(
  stage: PlanStageStats,
  base: Pick<TimelineRow, "id" | "label" | "y">,
  scale: Scale,
): TimelineRow {
  const barX = scale.x(scale.startMs);
  return {
    ...base,
    sub: stage.model ? clip(modelWithEffort(stage.model, stage.effort), SUB_CHARS) : "",
    skipped: false,
    barX,
    barWidth: 0,
    segments: [],
    duration: formatSpan(stage.execution_secs),
    durationX: barX + 8,
  };
}

function stageRow(stage: PlanStageStats, index: number, record: PlanStats, scale: Scale) {
  const y = TOP + index * ROW;
  const base = {
    id: stage.id,
    label: clip(stage.id, LABEL_CHARS),
    y,
    durationX: 0,
    duration: "",
  };
  if (isSkipped(stage)) {
    const barX = scale.x(scale.startMs);
    return { ...base, sub: "skipped", skipped: true, barX, barWidth: 0, segments: [] };
  }
  if (!stage.started_at && stage.phases.length === 0) return untimedRow(stage, base, scale);
  const start = Date.parse(stage.started_at ?? stage.phases[0]!.started_at);
  const phaseEnd = lastPhaseEnd(stage);
  const end = stage.completed_at
    ? Date.parse(stage.completed_at)
    : Number.isNaN(phaseEnd)
      ? Date.parse(record.ended_at)
      : phaseEnd;
  const barX = scale.x(start);
  const barEnd = scale.x(end);
  const segments = stage.phases
    .filter((span) => span.phase !== "waiting")
    .map((span, spanIndex) => {
      const from = Date.parse(span.started_at);
      const to = span.ended_at ? Date.parse(span.ended_at) : end;
      const x = scale.x(from);
      return {
        key: `${stage.id}-${spanIndex}`,
        phase: span.phase,
        x,
        width: Math.max(MIN_SEGMENT, scale.x(to) - x),
        title: `${PHASE_LABEL[span.phase]} ${clockHm(from)}–${clockHm(to)} (${formatSpan((to - from) / 1000)})`,
      };
    });
  return {
    ...base,
    sub: stage.model ? clip(modelWithEffort(stage.model, stage.effort), SUB_CHARS) : "",
    skipped: false,
    barX,
    barWidth: Math.max(0, barEnd - barX),
    segments,
    duration: formatSpan((end - start) / 1000),
    durationX: barEnd + 8,
  };
}

function ticksOf(scale: Scale): Tick[] {
  const stepMs =
    (TICK_STEPS.find((minutes) => scale.totalMs / (minutes * MINUTE_MS) <= MAX_TICKS) ??
      TICK_STEPS[TICK_STEPS.length - 1]!) * MINUTE_MS;
  const endX = scale.x(scale.startMs + scale.totalMs);
  const ticks: Tick[] = [];
  for (let offset = 0; offset <= scale.totalMs; offset += stepMs) {
    const x = scale.x(scale.startMs + offset);
    if (endX - x >= END_CLEARANCE || offset === 0) {
      ticks.push({ x, label: clockHm(scale.startMs + offset) });
    }
  }
  return ticks;
}

export function timelineLayout(record: PlanStats): TimelineLayout {
  const scale = scaleOf(record);
  const rows = record.stages.map((stage, index) => stageRow(stage, index, record, scale));
  const endMs = scale.startMs + scale.totalMs;
  const gridBottom = TOP + rows.length * ROW + 8;
  const repeat = rows.length > REPEAT_AXIS_ROWS;
  return {
    height: TOP + rows.length * ROW + BOTTOM + (repeat ? AXIS_REPEAT : 0),
    gridBottom,
    axisRepeatY: repeat ? gridBottom + 16 : null,
    rows,
    ticks: ticksOf(scale),
    end: { x: scale.x(endMs), label: clockHm(endMs) },
  };
}
