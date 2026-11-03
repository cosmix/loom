import {
  atom,
  type createStore,
  type PrimitiveAtom,
  type SetStateAction,
  type WritableAtom,
} from "jotai/vanilla";

import { z } from "@/api/zod";
import { safeStorage, storageKey } from "@/plan/chat/storage";
import { serverInfoAtom } from "@/plan/state/atoms";

export type Checkpoint = { commit: string | null; hashes: Record<string, string> };

const checkpointSchema = z.object({
  commit: z.string().nullable(),
  hashes: z.record(z.string(), z.string()),
});

type CheckpointState = WritableAtom<Checkpoint | null, [SetStateAction<Checkpoint | null>], void>;
type ProjectCheckpoint = { value: CheckpointState; generation: PrimitiveAtom<number> };

const checkpoints = new Map<string, ProjectCheckpoint>();

function checkpointFor(projectKey: string): ProjectCheckpoint {
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
  const project = { value: state, generation: atom(0) };
  checkpoints.set(projectKey, project);
  return project;
}

/** The checkpoint of the current project. Every write replaces it and moves its generation. */
export const checkpointAtom: CheckpointState = atom(
  (get) => {
    const key = get(serverInfoAtom)?.project_key;
    return key == null ? null : get(checkpointFor(key).value);
  },
  (get, set, update) => {
    const key = get(serverInfoAtom)?.project_key;
    if (key == null) return;
    const project = checkpointFor(key);
    set(project.generation, (n) => n + 1);
    set(project.value, update);
  },
);

/**
 * Counts replacements of the current project's checkpoint (first visit, Mark all seen) in this
 * store. A diff fetch or an acknowledgment captures it when it starts and writes nothing once it
 * has moved, so a response cut before a replacement never lands after it. `markSeen` leaves it.
 */
export const checkpointGenerationAtom = atom((get) => {
  const key = get(serverInfoAtom)?.project_key;
  return key == null ? 0 : get(checkpointFor(key).generation);
});

const seenHashAtom = atom(null, (get, set, path: string, hash: string | null) => {
  const key = get(serverInfoAtom)?.project_key;
  if (key == null) return;
  const { value } = checkpointFor(key);
  const checkpoint = get(value);
  // First-visit initialization belongs to the review UI.
  if (checkpoint === null) return;
  const hashes = { ...checkpoint.hashes };
  // A reviewed deletion removes the old hash rather than inventing a current one.
  if (hash === null) delete hashes[path];
  else hashes[path] = hash;
  set(value, { ...checkpoint, hashes });
});

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

/** Moves one document's seen hash (null: seen as removed), keeping the checkpoint's generation. */
export function markSeen(
  store: ReturnType<typeof createStore>,
  path: string,
  hash: string | null,
): void {
  store.set(seenHashAtom, path, hash);
}
