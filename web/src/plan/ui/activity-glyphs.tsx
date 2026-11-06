import { BellRing } from "lucide-react";
import type { JSX, ReactNode } from "react";

import type { GlyphProps } from "@/plan/ui/glyphs";
import { ArrowSpinner } from "@/plan/ui/spinner";

import "./activity-glyphs.css";

export type ActivityKind =
  | "summaries"
  | "pressure"
  | "codex"
  | "drafting"
  | "running"
  | "starting"
  | "waiting"
  | "permission";

export const ACTIVITY_KINDS: readonly ActivityKind[] = [
  "permission",
  "waiting",
  "running",
  "pressure",
  "codex",
  "drafting",
  "summaries",
  "starting",
];

export const ACTIVITY_LABELS: Record<ActivityKind, string> = {
  permission: "Needs permission",
  waiting: "Waiting for you",
  running: "Plan running",
  pressure: "Pressure test",
  codex: "Codex reviewing",
  drafting: "Drafting a document",
  summaries: "Rebuilding summaries",
  starting: "Session starting",
};

/**
 * The kinds that mean work is under way; the rest (`YourTurnKind`) mean "your turn": the waiting
 * kind draws the status dot, the permission kind its bell. A progress kind's mark follows
 * `ui/DESIGN.md`, Status: the rotating arrows (`StateMark`) before a state word, the dot
 * (`ActivityGlyph`) before the item or action itself.
 */
export const PROGRESS_KINDS = [
  "running",
  "pressure",
  "codex",
  "drafting",
  "summaries",
  "starting",
] as const satisfies readonly ActivityKind[];

export type ProgressKind = (typeof PROGRESS_KINDS)[number];
export type YourTurnKind = Exclude<ActivityKind, ProgressKind>;

/** The kinds whose label is a state word ("Plan running", "Rebuilding summaries"): they draw `StateMark`. */
export const STATE_WORD_KINDS = [
  "running",
  "codex",
  "drafting",
  "summaries",
  "starting",
] as const satisfies readonly ProgressKind[];

export type StateWordKind = (typeof STATE_WORD_KINDS)[number];

export function isStateWordKind(kind: ActivityKind): kind is StateWordKind {
  return (STATE_WORD_KINDS as readonly ActivityKind[]).includes(kind);
}

export function isProgressKind(kind: ActivityKind): kind is ProgressKind {
  return (PROGRESS_KINDS as readonly ActivityKind[]).includes(kind);
}

/**
 * A running plan's glyph before the plan itself (`ui/glyphs.tsx` `PLAN_STATE_GLYPHS.running`): the
 * live status dot, with the props a glyph site passes a lucide icon. Before the word "Running" the
 * mark is the arrows (`PLAN_STATE_WORD_GLYPHS`).
 */
export function LiveDot({ className, size }: GlyphProps): JSX.Element {
  return (
    <span
      className={`activity-glyph${className ? ` ${className}` : ""}`}
      data-kind="running"
      style={size === undefined ? undefined : { width: size, height: size }}
      aria-hidden="true"
    >
      <span className="activity-glyph__led" />
    </span>
  );
}

/**
 * The mark that precedes a state word ("Running", "Drafting", "Rebuilding"): the rotating arrows
 * (`ui/spinner.tsx`), which rest and dim when `paused`. Decorative: the word beside it is the
 * announcement.
 */
export function StateMark({
  kind,
  className,
  paused = false,
}: {
  kind: ActivityKind;
  className?: string;
  paused?: boolean;
}): JSX.Element {
  return <ArrowSpinner data-kind={kind} busy={!paused} className={className} />;
}

/**
 * The decorative glyph of an activity kind that precedes the item or action itself (a card's
 * title, an action label). The root is `aria-hidden`: the caller's label is the announcement.
 * Every kind but permission is a status dot, live (green, pulsing) while it runs and gray while
 * paused; permission is the bell. Before a state word draw `StateMark`; the roundel is the header
 * pill's alone (`header/activity.tsx`).
 */
export function ActivityGlyph({
  kind,
  className,
  paused = false,
}: {
  kind: ActivityKind;
  className?: string;
  paused?: boolean;
}): JSX.Element {
  const root = {
    className: `activity-glyph${className ? ` ${className}` : ""}`,
    "data-kind": kind,
    "data-paused": paused || undefined,
  };
  return (
    <span {...root} aria-hidden="true">
      {kind === "permission" ? (
        <BellRing className="activity-glyph__motion" />
      ) : (
        <span className="activity-glyph__led" />
      )}
    </span>
  );
}

export type ActivityTone = "executing" | "ring" | "completed" | "warning" | "neutral";

export const ACTIVITY_TONES: Record<ActivityKind, ActivityTone> = {
  running: "executing",
  summaries: "executing",
  codex: "executing",
  starting: "executing",
  pressure: "ring",
  drafting: "ring",
  waiting: "completed",
  permission: "warning",
};

/**
 * The glyph cell that leads an activity row (a 20px column, no fill of its own). A kind gives its
 * tone and its glyph: `StateMark` for a state-word kind, the dot or bell otherwise. `children`
 * replace the glyph, and a tile with neither kind draws in the muted tone.
 */
export function ActivityTile({
  kind,
  paused = false,
  children,
}: {
  kind?: ActivityKind;
  paused?: boolean;
  children?: ReactNode;
}): JSX.Element {
  return (
    <span
      className="activity-tile"
      data-tone={kind ? ACTIVITY_TONES[kind] : "neutral"}
      aria-hidden="true"
    >
      {children ??
        (kind &&
          (isStateWordKind(kind) ? (
            <StateMark kind={kind} paused={paused} />
          ) : (
            <ActivityGlyph kind={kind} paused={paused} />
          )))}
    </span>
  );
}
