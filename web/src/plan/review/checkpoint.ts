import { atom, type createStore, type SetStateAction, type WritableAtom } from "jotai/vanilla";

import { z } from "@/api/zod";
import { safeStorage, storageKey } from "@/plan/chat/storage";
import { serverInfoAtom } from "@/plan/state/atoms";

export type Checkpoint = { commit: string | null; hashes: Record<string, string> };

const checkpointSchema = z.object({
  commit: z.string().nullable(),
  hashes: z.record(z.string(), z.string()),
});

type CheckpointState = WritableAtom<Checkpoint | null, [SetStateAction<Checkpoint | null>], void>;

const checkpoints = new Map<string, CheckpointState>();

function checkpointFor(projectKey: string): CheckpointState {
  const existing = checkpoints.get(projectKey);
  if (existing) return existing;

  const key = storageKey(projectKey, "checkpoint");
  // The wrapper distinguishes an explicit reset from a checkpoint not yet read.
  const base = atom<{ value: Checkpoint | null } | null>(null);
  const state: CheckpointState = atom(
    (get) => {
      const cached = get(base);
      if (cached !== null) return cached.value;
      const result = checkpointSchema.safeParse(safeStorage.getItem<unknown>(key, null));
      return result.success ? result.data : null;
    },
    (get, set, update) => {
      const next = typeof update === "function" ? update(get(state)) : update;
      set(base, { value: next });
      safeStorage.setItem(key, next);
    },
  );
  checkpoints.set(projectKey, state);
  return state;
}

export const checkpointAtom: CheckpointState = atom(
  (get) => {
    const key = get(serverInfoAtom)?.project_key;
    return key == null ? null : get(checkpointFor(key));
  },
  (get, set, update) => {
    const key = get(serverInfoAtom)?.project_key;
    if (key != null) set(checkpointFor(key), update);
  },
);

export function changedSince(
  cp: Checkpoint,
  current: Record<string, string>,
): Array<{ path: string; change: "added" | "changed" | "removed" }> {
  const changes: ReturnType<typeof changedSince> = [];
  const paths = new Set([...Object.keys(cp.hashes), ...Object.keys(current)]);
  for (const path of [...paths].sort()) {
    if (!Object.hasOwn(current, path)) {
      changes.push({ path, change: "removed" });
    } else if (!Object.hasOwn(cp.hashes, path)) {
      changes.push({ path, change: "added" });
    } else if (cp.hashes[path] !== current[path]) {
      changes.push({ path, change: "changed" });
    }
  }
  return changes;
}

export function markSeen(store: ReturnType<typeof createStore>, path: string, hash: string): void {
  const checkpoint = store.get(checkpointAtom);
  // First-visit initialization belongs to the review UI.
  if (checkpoint === null) return;
  store.set(checkpointAtom, {
    ...checkpoint,
    hashes: { ...checkpoint.hashes, [path]: hash },
  });
}
