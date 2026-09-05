import statuses from "@/api/fixtures/statuses.json";
import type { StageStatus } from "@/api/schema";

/// Metadata shared by state badges and the ledger legend.
export type StageStateMeta = {
  status: StageStatus;
  icon: string;
  label: string;
  legend: string;
};

const STATUS_TABLE = statuses as readonly StageStateMeta[];

/// Stage metadata keyed by its wire-format status.
export const STAGE_STATES: Record<StageStatus, StageStateMeta> = Object.fromEntries(
  STATUS_TABLE.map((entry) => [entry.status, entry]),
) as Record<StageStatus, StageStateMeta>;

/// Stage metadata in the authoritative Rust ledger order.
export const LEGEND: readonly StageStateMeta[] = STATUS_TABLE;
