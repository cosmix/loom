import { useAtomValue } from "jotai";
import { useEffect } from "react";
import { useNavigate } from "react-router";

import type { ChecklistView, StandalonePlanView } from "@/plan/api/schema";
import type { KickoffView } from "@/plan/api/schema-kickoff";
import { toAddress } from "@/plan/files/address";
import { featuresAtom, standaloneAtom } from "@/plan/state/atoms";

const landed = new Set<string>();

export function resetKickoffLanding(): void {
  landed.clear();
}

function landingPath(
  view: KickoffView,
  features: ChecklistView[],
  plans: StandalonePlanView[],
): string | null {
  if (view.target === null) return null;
  if (view.shape.size === "feature") {
    const name = /^doc\/product\/([^/]+)\/index\.md$/.exec(view.target)?.[1];
    return name && features.some((feature) => feature.feature === name)
      ? `/f/${encodeURIComponent(name)}`
      : null;
  }
  if (view.shape.size !== "plan") return null;
  const filename = /^doc\/plans\/(?:(?:IN_PROGRESS|DONE|RETIRED)-)?(PLAN-[^/]+\.md)$/.exec(
    view.target,
  )?.[1];
  if (!filename) return null;
  // Lifecycle renames retain the document's identity, including retired plans in archive/.
  const prefixes = ["", "IN_PROGRESS-", "DONE-", "RETIRED-"];
  const plan = plans.find(({ path }) =>
    prefixes.some((prefix) => path.endsWith(`/${prefix}${filename}`)),
  );
  return plan ? toAddress(plan.path) : null;
}

/** Pass null unless the view belongs to the kickoff session active in this route. */
export function useKickoffLanding(view: KickoffView | null): void {
  const features = useAtomValue(featuresAtom);
  const plans = useAtomValue(standaloneAtom);
  const navigate = useNavigate();
  useEffect(() => {
    if (view?.target == null) return;
    const key = `${view.session}:${view.target}`;
    if (landed.has(key)) return;
    const path = landingPath(view, features, plans);
    if (path === null) return;
    landed.add(key);
    void navigate(path, { replace: true });
  }, [view, features, plans, navigate]);
}
