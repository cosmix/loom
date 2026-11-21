import { atom } from "jotai/vanilla";

import type { PlanStats } from "@/api/schema";
import { snapshotAtom } from "@/state/atoms";

/// The finished-plan record while the daemon reports one, else null. A derived atom, so the
/// shell re-renders when the record changes and not on every snapshot.
export const planStatsAtom = atom<PlanStats | null>(
  (get) => get(snapshotAtom)?.status.plan_stats ?? null,
);
