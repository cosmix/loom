import { BellRing } from "lucide-react";
import type { JSX, ReactNode } from "react";

import { BusyRoundel } from "@/aurora-ui/feedback/BusyRoundel";

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
 * The kinds that mean work is in progress. They all draw the one roundel (`ui/DESIGN.md`, Status);
 * the rest (`YourTurnKind`) mean "your turn" and keep their own glyphs.
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

function YourTurnShape({ kind }: { kind: YourTurnKind }): JSX.Element {
  switch (kind) {
    case "waiting":
      return <span className="activity-glyph__led" />;
    case "permission":
      return <BellRing className="activity-glyph__motion" />;
  }
}

function ActivityShape({ kind, paused }: { kind: ActivityKind; paused: boolean }): JSX.Element {
  if (!isProgressKind(kind)) return <YourTurnShape kind={kind} />;
  // A paused roundel rests upright and dims (busy-roundel.css); it is not frozen mid-turn.
  return <BusyRoundel busy={!paused} size="100%" busyLabel="" idleLabel="" />;
}

/**
 * The decorative glyph of an activity kind. The root is `aria-hidden`: the caller's label is the
 * announcement. Work in progress is the roundel; a paused one rests.
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
  return (
    <span
      className={`activity-glyph${className ? ` ${className}` : ""}`}
      data-kind={kind}
      data-paused={paused || undefined}
      aria-hidden="true"
    >
      <ActivityShape kind={kind} paused={paused} />
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
