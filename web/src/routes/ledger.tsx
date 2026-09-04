import { ActivityPanel } from "@/components/activity-panel";
import { AlertsBand } from "@/components/alerts-band";
import { AttentionPanel } from "@/components/attention-panel";
import { LedgerTable } from "@/components/ledger-table";

/// Route `/`: the TUI's vertical order — alerts, ledger, attention, activity —
/// with the two panels side by side once there is room.
export function Ledger() {
  return (
    <div className="flex flex-col gap-6">
      <AlertsBand />
      <LedgerTable />
      <div className="grid gap-6 lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
        <AttentionPanel />
        <ActivityPanel />
      </div>
    </div>
  );
}
