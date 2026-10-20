import { useState, type ReactNode } from "react";
import { createStore, Provider } from "jotai";
import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type Snapshot } from "@/api/schema";
import { snapshotAtom, connectionAtom, type ConnectionPhase } from "@/state/atoms";
import { TooltipProvider } from "@/components/ui/tooltip";

// Isolated sample state: the examples never subscribe to or mutate a live run.
export const HELP_SNAPSHOT = snapshotSchema.parse({ ...fixture, terminals: true });
// Preserve the five-minute mean while showing bursts rather than a flat test series.
const BURSTS = [0.5, 0.7, 1.1, 1.5, 0.9, 0.6, 1.3, 1.2, 1.4, 0.8];
if (HELP_SNAPSHOT.status.tokens) {
  const flow = HELP_SNAPSHOT.status.tokens;
  for (const stage of flow.stages) {
    stage.series = stage.series.map((value, i) => Math.round(value * BURSTS[i % BURSTS.length]));
    for (const agent of stage.agents) {
      agent.series = agent.series.map((value, i) => Math.round(value * BURSTS[i % BURSTS.length]));
    }
  }
  flow.plan.series = flow.plan.series.map((_, i) =>
    flow.stages.reduce((total, stage) => total + (stage.series[i] ?? 0), 0),
  );
}
export const HELP_STAGE = {
  ...HELP_SNAPSHOT.status.stages.find((stage) => stage.status === "executing")!,
  session_backend: "tmux" as const,
};

export function HelpExample({
  children,
  snapshot = HELP_SNAPSHOT,
  phase = "live",
}: {
  children: ReactNode;
  snapshot?: Snapshot;
  phase?: ConnectionPhase;
}) {
  const [store] = useState(() => {
    const example = createStore();
    example.set(snapshotAtom, snapshot);
    example.set(connectionAtom, { phase, since: Date.now() - 5000 });
    return example;
  });
  return (
    <Provider store={store}>
      <TooltipProvider>{children}</TooltipProvider>
    </Provider>
  );
}
