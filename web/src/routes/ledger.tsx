import { useAtomValue } from "jotai/react";

import { ActivityPanel } from "@/components/activity-panel";
import { AlertsBand } from "@/components/alerts-band";
import { AttentionPanel } from "@/components/attention-panel";
import { LedgerTable } from "@/components/ledger-table";
import { attentionAtom } from "@/state/atoms";

/// Route `/ledger`: the TUI's vertical order — alerts, ledger, attention,
/// activity — with the two panels side by side once there is room.
export function Ledger() {
  const hasAttention = useAtomValue(attentionAtom).length > 0;

  return (
    <div className="mx-auto flex w-full max-w-[1440px] flex-col gap-6 px-4 py-5 sm:px-6">
      <AlertsBand />
      <LedgerTable />
      <div
        className={
          hasAttention ? "grid gap-6 lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]" : undefined
        }
      >
        <AttentionPanel />
        <ActivityPanel wide={!hasAttention} />
      </div>
    </div>
  );
}
