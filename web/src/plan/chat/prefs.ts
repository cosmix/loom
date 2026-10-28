import { atom } from "jotai/vanilla";
import { atomWithStorage } from "jotai/utils";

import { z } from "@/api/zod";
import { serverInfoAtom } from "@/plan/state/atoms";
import { storageKey, validatedStorage } from "./storage";

const MIN_WIDTH = 20;
const MAX_WIDTH = 40;
const DEFAULT_WIDTH = 26;

type Update<Value> = Value | ((previous: Value) => Value);

/**
 * A per-project preference kept in validated localStorage. Before the first hello there is no
 * project key, so the value lives in memory only rather than under a key shared by every project
 * on the origin.
 */
function projectPref<Value>(name: string, fallback: Value, schema: z.ZodType<Value>) {
  const storage = validatedStorage(schema);
  const keyed = new Map<string, ReturnType<typeof atomWithStorage<Value>>>();
  const unkeyed = atom(fallback);
  const targetFor = (projectKey: string | undefined) => {
    if (projectKey == null) return unkeyed;
    let value = keyed.get(projectKey);
    if (!value) {
      value = atomWithStorage<Value>(storageKey(projectKey, name), fallback, storage, {
        getOnInit: true,
      });
      keyed.set(projectKey, value);
    }
    return value;
  };
  return atom(
    (get) => get(targetFor(get(serverInfoAtom)?.project_key)),
    (get, set, update: Update<Value>) => {
      const target = targetFor(get(serverInfoAtom)?.project_key);
      const next =
        typeof update === "function" ? (update as (previous: Value) => Value)(get(target)) : update;
      set(target, next);
    },
  );
}

// A stored width outside the range is clamped; anything that is not a number reads as the default.
const widthAtom = projectPref(
  "panel-width",
  DEFAULT_WIDTH,
  z.number().transform((width) => Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, width))),
);
const collapsedAtom = projectPref("panel-collapsed", false, z.boolean());

export const panelWidthAtom = atom(
  (get) => get(widthAtom),
  (get, set, update: Update<number>) => {
    const width = typeof update === "function" ? update(get(widthAtom)) : update;
    set(
      widthAtom,
      Number.isFinite(width) ? Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, width)) : DEFAULT_WIDTH,
    );
  },
);

export const panelCollapsedAtom = atom(
  (get) => get(collapsedAtom),
  (_get, set, update: Update<boolean>) => set(collapsedAtom, update),
);

/** Set once the first-hold note has been shown, per project. */
export const holdNoteSeenAtom = projectPref("hold-note-seen", false, z.boolean());
