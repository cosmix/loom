import { useAtomValue } from "jotai/react";
import { atom, type PrimitiveAtom } from "jotai/vanilla";

import { z } from "@/api/zod";
import { apiGet, apiPost, type ApiResult } from "@/plan/api/client";
import {
  pressureOptionsSchema,
  pressureRunSchema,
  pressureSlotsSchema,
  type PressureOptions,
  type PressureRunView,
} from "@/plan/api/schema-pressure";
import { planSocketAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";

export const pressureStartSchema = z.strictObject({
  plan: z.string(),
  ...pressureSlotsSchema.shape,
});
export type PressureStart = z.infer<typeof pressureStartSchema>;

export const pressureRunAtom: PrimitiveAtom<PressureRunView | null> = atom<PressureRunView | null>(
  null,
);
export const pressureOptionsAtom: PrimitiveAtom<ApiResult<PressureOptions> | null> =
  atom<ApiResult<PressureOptions> | null>(null);
export const pressureRunWaitingAtom = atom(false);

const stoppingSchema = z.strictObject({ stopping: z.literal(true) });
const clearedSchema = z.strictObject({ cleared: z.literal(true) });

export function fetchPressureOptions(): Promise<ApiResult<PressureOptions>> {
  return apiGet("/api/pressure/options", pressureOptionsSchema);
}

export function startPressure(body: PressureStart): Promise<ApiResult<PressureRunView>> {
  return apiPost("/api/pressure/start", body, pressureRunSchema, { contentType: "json" });
}

export function stopPressure(): Promise<ApiResult<z.infer<typeof stoppingSchema>>> {
  return apiPost("/api/pressure/stop", "", stoppingSchema, { contentType: "text" });
}

export function confirmConfig(): Promise<ApiResult<z.infer<typeof clearedSchema>>> {
  return apiPost("/api/pressure/confirm-config", "", clearedSchema, { contentType: "text" });
}

function refreshPressureRun(store: Store): void {
  // There is no run GET: pressure/install.rs sends last_view after each hello. Keep the
  // visible run until that authoritative pressure_run arrives, including across resyncs.
  store.set(pressureRunWaitingAtom, store.get(pressureRunAtom) !== null);
}

// Per-store option request generation: a response is applied only while it is the latest
// request, and detaching the subscription bumps it so late responses are dropped.
const optionsRequests = new WeakMap<Store, number>();

function nextOptionsRequest(store: Store): number {
  const next = (optionsRequests.get(store) ?? 0) + 1;
  optionsRequests.set(store, next);
  return next;
}

/** Fetches the options and stores them unless a newer request or a detach superseded this one. */
export async function refreshPressureOptions(store: Store): Promise<void> {
  const current = nextOptionsRequest(store);
  const result = await fetchPressureOptions();
  if (current === optionsRequests.get(store)) store.set(pressureOptionsAtom, result);
}

/** Install once per store; follows socket replacement and ignores detached HTTP responses. */
export function installPressureSubscription(store: Store): () => void {
  let detach = () => {};
  const reload = () => {
    void refreshPressureOptions(store);
  };
  const attach = () => {
    detach();
    detach = () => {};
    nextOptionsRequest(store);
    const socket = store.get(planSocketAtom);
    if (!socket) return;
    const unsubscribe = socket.subscribe("pressure_run", ({ type: _type, ...run }) => {
      store.set(pressureRunAtom, run);
      store.set(pressureRunWaitingAtom, false);
    });
    const stopResync = socket.onResync(() => {
      refreshPressureRun(store);
      reload();
    });
    detach = () => {
      unsubscribe();
      stopResync();
    };
    reload();
  };
  attach();
  const stopSocket = store.sub(planSocketAtom, attach);
  return () => {
    nextOptionsRequest(store);
    detach();
    stopSocket();
  };
}

export function usePressureRun(): PressureRunView | null {
  return useAtomValue(pressureRunAtom);
}

/** Options loaded by installPressureSubscription, including reconnect results. */
export function usePressureOptions(): ApiResult<PressureOptions> | null {
  return useAtomValue(pressureOptionsAtom);
}
