import {
  BanIcon,
  CheckIcon,
  CircleIcon,
  CircleQuestionMarkIcon,
  CircleXIcon,
  PauseIcon,
  PlayIcon,
  RotateCwIcon,
  ScaleIcon,
  TriangleAlertIcon,
  XIcon,
  ZapIcon,
  type LucideIcon,
} from "lucide-react";

import statuses from "@/api/fixtures/statuses.json";
import type { StageStatus } from "@/api/schema";

/// Metadata shared by state badges and the ledger legend.
export type StageStateMeta = {
  status: StageStatus;
  icon: LucideIcon;
  label: string;
  legend: string;
};

const STATE_ICONS: Record<StageStatus, LucideIcon> = {
  "waiting-for-deps": CircleIcon,
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
  "needs-adjudication": ScaleIcon,
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
