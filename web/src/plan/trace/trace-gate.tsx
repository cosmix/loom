import { useStore } from "jotai/react";
import type { ReactNode } from "react";

import type { TraceView } from "@/plan/api/schema-trace";
import { useFeature } from "@/plan/feature/use-feature";
import { LoadState } from "@/plan/ui/load-state";

import { loadTrace, useTrace } from "./state";

/**
 * The load states of a page that reads the feature's trace: the feature still loading, the trace
 * loading, or the trace failing to load with no earlier view to keep. Once a view is loaded
 * `children` draws it, and a failed refetch keeps showing the loaded view (`state.ts`).
 * `noun` names what the page shows ("the coverage").
 */
export function TraceGate(props: {
  noun: string;
  children: (trace: TraceView, feature: string) => ReactNode;
}) {
  const store = useStore();
  const { checklist } = useFeature();
  const feature = checklist?.feature ?? "";
  const { trace, error } = useTrace(feature);
  if (!checklist) return <LoadState state="loading" title="Loading the feature" />;
  if (trace !== null) return props.children(trace, feature);
  if (error === null) return <LoadState state="loading" title={`Loading ${props.noun}`} />;
  return (
    <LoadState
      state="error"
      title={`Could not load ${props.noun}`}
      message={error}
      action={{ label: "Try again", onClick: () => void loadTrace(store, feature) }}
    />
  );
}
