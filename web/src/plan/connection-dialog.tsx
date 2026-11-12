import { useAtomValue } from "jotai/react";

import { HazardPanel } from "@/aurora-ui/feedback/HazardPanel";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { connectionAtom, unauthorizedAtom } from "@/plan/state/atoms";

/** Keep the stopped planner visible on every route while the socket retries. */
export function PlannerConnectionDialog() {
  const connection = useAtomValue(connectionAtom);
  const unauthorized = useAtomValue(unauthorizedAtom);
  const unavailable = !unauthorized && connection.phase === "reconnecting";
  return (
    <Dialog open={unavailable}>
      <DialogContent
        role="alertdialog"
        showCloseButton={false}
        className="overflow-hidden p-0 sm:max-w-md"
        onEscapeKeyDown={(event) => event.preventDefault()}
        onInteractOutside={(event) => event.preventDefault()}
      >
        <HazardPanel tone="warning" title="Planner unavailable" bodyClassName="space-y-3">
          <DialogTitle className="leading-snug">Loom feature planner is not responding</DialogTitle>
          <DialogDescription className="leading-relaxed">
            The connection to the planner was lost. If it has stopped, run loom plan in this project
            to start it again.
          </DialogDescription>
          <p className="text-sm text-muted-foreground" role="status">
            Reconnecting automatically. This message will close when the connection returns.
          </p>
        </HazardPanel>
      </DialogContent>
    </Dialog>
  );
}
