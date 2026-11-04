import { useAtomValue } from "jotai/react";
import type { JSX } from "react";

import { useNow } from "@/components/hooks/use-now";
import { QuotaMeters } from "@/components/quota-meters";
import { providerRows, type ProviderName } from "@/lib/quota";
import { summaryPanelAtom } from "@/plan/summaries/state";

const PROVIDERS: ReadonlyArray<{ name: ProviderName; label: string }> = [
  { name: "claude", label: "Claude" },
  { name: "codex", label: "Codex" },
];

/**
 * The provider limits, as the dashboard's footer draws them. The planner has no `quota_left` data,
 * so the meters take none. A provider the server reports nothing for is named, not left out
 * silently. The feed (`SummaryFeedHost`) fills the panel; until it has answered the footer is empty
 * rather than claiming limits are unavailable.
 */
export function PlanFooter(): JSX.Element | null {
  const panel = useAtomValue(summaryPanelAtom);
  const nowSecs = Math.floor(useNow() / 1000);
  if (panel === null) return null;
  const reporting = new Set(providerRows(panel.quota).map(([name]) => name));
  const missing = PROVIDERS.filter(({ name }) => !reporting.has(name));

  return (
    <footer className="plan-footer">
      <QuotaMeters snapshot={panel.quota} quotaLeft={[]} nowSecs={nowSecs} />
      {missing.map(({ name, label }) => (
        <span
          key={name}
          className="plan-footer__note text-[length:var(--type-fine)] text-muted-foreground"
        >
          {`${label} limits unavailable`}
        </span>
      ))}
    </footer>
  );
}
