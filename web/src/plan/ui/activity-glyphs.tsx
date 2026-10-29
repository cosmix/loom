import { BellRing, LoaderCircle, PenLine, Play, RefreshCw } from "lucide-react";
import type { JSX } from "react";

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

function PressureGauge(): JSX.Element {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
      <path d="M3 15a9 9 0 0 1 18 0" strokeLinecap="round" />
      <path className="activity-glyph__needle" d="M12 15V8" strokeLinecap="round" />
      <circle cx={12} cy={15} r={1.5} fill="currentColor" stroke="none" />
    </svg>
  );
}

function CodexScan(): JSX.Element {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M8 3H3v5M16 3h5v5M21 16v5h-5M8 21H3v-5" />
      <path className="activity-glyph__beam" d="M12 6v12" />
    </svg>
  );
}

function ActivityShape({ kind }: { kind: ActivityKind }): JSX.Element {
  switch (kind) {
    case "summaries":
      return <RefreshCw className="activity-glyph__motion" />;
    case "pressure":
      return <PressureGauge />;
    case "codex":
      return <CodexScan />;
    case "drafting":
      return <PenLine className="activity-glyph__motion" />;
    case "running":
      return (
        <>
          <span className="activity-glyph__pulse" />
          <Play />
        </>
      );
    case "starting":
      return <LoaderCircle className="activity-glyph__motion" />;
    case "waiting":
      return <span className="activity-glyph__led" />;
    case "permission":
      return <BellRing className="activity-glyph__motion" />;
  }
}

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
      <ActivityShape kind={kind} />
    </span>
  );
}
