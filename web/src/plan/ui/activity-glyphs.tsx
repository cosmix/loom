import { BellRing } from "lucide-react";
import type { JSX, ReactNode } from "react";

import type { GlyphProps } from "@/plan/ui/glyphs";

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
 * The kinds that mean work is under way. Each is a state being shown, so each draws the live
 * status dot (`ui/DESIGN.md`, Status); the rest (`YourTurnKind`) mean "your turn": the waiting
 * kind draws the same dot, the permission kind its bell.
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

export function isProgressKind(kind: ActivityKind): kind is ProgressKind {
  return (PROGRESS_KINDS as readonly ActivityKind[]).includes(kind);
}

/**
 * A running plan's state glyph (`ui/glyphs.tsx` `PLAN_STATE_GLYPHS.running`): the live status dot,
 * with the props a glyph site passes a lucide icon.
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
 * The decorative glyph of an activity kind. The root is `aria-hidden`: the caller's label is the
 * announcement. Every kind but permission is a status dot, live (green, pulsing) while it runs and
 * gray while paused; permission is the bell. It never draws the roundel, which is the header
 * pill's alone (`header/activity.tsx`), nor a spinner: an operation that ends draws the arrow
 * spinner (`ui/spinner.tsx`) at its own site.
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
 * The glyph cell that leads an activity row (a 20px column, no fill of its own). A kind gives its glyph and tone; `children` replace
 * the glyph, and a tile with neither kind draws in the muted tone.
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
      {children ?? (kind && <ActivityGlyph kind={kind} paused={paused} />)}
    </span>
  );
}
