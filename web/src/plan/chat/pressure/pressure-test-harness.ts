import type { PressureRunView } from "@/plan/api/schema-pressure";
import type { connectPlanSocket } from "@/plan/api/socket";
import { planSocketAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";

export function socketHarness(store: Store) {
  const listeners = new Map<string, Set<(message: unknown) => void>>();
  const resyncs = new Set<() => void>();
  const socket = {
    send: () => true,
    dispose: () => {},
    subscribe: (type: string, handler: (message: unknown) => void) => {
      const handlers = listeners.get(type) ?? new Set();
      listeners.set(type, handlers);
      handlers.add(handler);
      return () => void handlers.delete(handler);
    },
    onResync: (handler: () => void) => {
      resyncs.add(handler);
      return () => void resyncs.delete(handler);
    },
  };
  store.set(planSocketAtom, socket as unknown as ReturnType<typeof connectPlanSocket>);
  return {
    receive: (view: PressureRunView) => {
      listeners
        .get("pressure_run")
        ?.forEach((handler) => handler({ type: "pressure_run", ...view }));
    },
    resync: () => resyncs.forEach((handler) => handler()),
    optionsChanged: () =>
      listeners
        .get("pressure_options_changed")
        ?.forEach((handler) => handler({ type: "pressure_options_changed" })),
  };
}
