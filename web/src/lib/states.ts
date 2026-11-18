import {
  BanIcon,
  CheckIcon,
  CircleAlertIcon,
  CircleIcon,
  CircleQuestionMarkIcon,
  CircleXIcon,
  HourglassIcon,
  OctagonAlertIcon,
  PauseIcon,
  PlayIcon,
  RotateCwIcon,
  SplitIcon,
  TriangleAlertIcon,
  XIcon,
  ZapIcon,
  type LucideIcon,
} from "lucide-react";

import statuses from "@/api/fixtures/statuses.json";
import type { StageStatus } from "@/api/schema";
import type { CompletionDisplayState, DisplayState } from "@/lib/display-state";

/// Metadata shared by state badges and the ledger legend.
export type StageStateMeta = {
  status: DisplayState;
  icon: LucideIcon;
  label: string;
  legend: string;
};

const STATE_ICONS: Record<StageStatus, LucideIcon> = {
  "waiting-for-deps": HourglassIcon,
  queued: PlayIcon,
  executing: CircleIcon,
  "waiting-for-input": CircleQuestionMarkIcon,
  "needs-handoff": RotateCwIcon,
  completed: CheckIcon,
  skipped: BanIcon,
  blocked: XIcon,
  "completed-with-failures": TriangleAlertIcon,
  "merge-conflict": ZapIcon,
  "merge-blocked": CircleXIcon,
  "needs-human-review": PauseIcon,
  "needs-adjudication": SplitIcon,
};

// Labels and order follow the Rust fixture; dashboard icons use Lucide.
const STATUS_TABLE: readonly StageStateMeta[] = statuses.map((entry) => ({
  ...entry,
  status: entry.status as StageStatus,
  icon: STATE_ICONS[entry.status as StageStatus],
}));

/// Stage metadata keyed by its wire-format status.
export const STAGE_STATES: Record<StageStatus, StageStateMeta> = Object.fromEntries(
  STATUS_TABLE.map((entry) => [entry.status, entry]),
) as Record<StageStatus, StageStateMeta>;

/// Stage metadata in the authoritative Rust ledger order.
export const LEGEND: readonly StageStateMeta[] = STATUS_TABLE;

/// The states a completion blocker draws over an executing stage, labelled
/// as `loom status` labels their attention entries.
const COMPLETION_STATES: Record<CompletionDisplayState, StageStateMeta> = {
  "completion-pending": {
    status: "completion-pending",
    icon: CircleAlertIcon,
    label: "Completion pending",
    legend:
      "executing, but loom stage complete failed a gate; fix or dispute it, a repeat of the same failure parks the stage",
  },
  "completion-blocked": {
    status: "completion-blocked",
    icon: OctagonAlertIcon,
    label: "Completion blocked",
    legend:
      "executing, but the same completion failure repeated; loom stops the writer and parks the stage for review",
  },
};

/// Every state the dashboard draws, keyed by `displayState`.
export const DISPLAY_STATES: Record<DisplayState, StageStateMeta> = {
  ...STAGE_STATES,
  ...COMPLETION_STATES,
};

/// The legend with the completion states after the wire states that need you.
export const DISPLAY_LEGEND: readonly StageStateMeta[] = [
  ...LEGEND,
  ...Object.values(COMPLETION_STATES),
];
