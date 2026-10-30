import { atom, type PrimitiveAtom } from "jotai";
import type { createStore, Getter, SetStateAction, WritableAtom } from "jotai/vanilla";
import { z } from "@/api/zod";
import { storageKey } from "@/plan/chat/storage";
import { serverInfoAtom } from "@/plan/state/atoms";
import type { FoldNote, OpenRun } from "./types";

const openRunsSchema = z.array(
  z.object({
    level: z.union([z.literal(0), z.literal(1), z.literal(2)]),
    sentenceId: z.string(),
    restsOn: z.array(
      z.object({ path: z.string(), anchor: z.string(), title: z.string().optional() }),
    ),
    parent: z.string().nullable(),
  }),
);

type OpenRunsAtom = WritableAtom<OpenRun[], [SetStateAction<OpenRun[]>], void>;

const openRuns = new Map<string, OpenRunsAtom>();
const notes = new Map<string, PrimitiveAtom<FoldNote[]>>();

/** The project segment of the key before the first hello, which names the project. */
const UNKEYED = "unkeyed";

/**
 * Where a page's open runs are kept: per project, so a tab reused for another project cannot
 * restore the first project's page of the same name. A mount with no hello (the frozen contract
 * "concept view state survives a reload through session storage" renders one) shares the
 * `unkeyed` segment. Revisit when that contract mounts a hello: `projectPref` keeps such a value
 * in memory instead.
 */
function storedKey(get: Getter, pageKey: string): string {
  return storageKey(get(serverInfoAtom)?.project_key ?? UNKEYED, `concept:${pageKey}`);
}

function readStored(key: string): OpenRun[] {
  try {
    const stored = sessionStorage.getItem(key);
    if (stored === null) return [];
    const result = openRunsSchema.safeParse(JSON.parse(stored));
    return result.success ? result.data : [];
  } catch {
    return [];
  }
}

export function openRunsAtom(pageKey: string): OpenRunsAtom {
  const existing = openRuns.get(pageKey);
  if (existing) return existing;

  const base = atom<OpenRun[] | null>(null);
  const state: OpenRunsAtom = atom(
    (get) => get(base) ?? readStored(storedKey(get, pageKey)),
    (get, set, update) => {
      const next = typeof update === "function" ? update(get(state)) : update;
      set(base, next);
      try {
        sessionStorage.setItem(storedKey(get, pageKey), JSON.stringify(next));
      } catch {
        // The in-memory state remains usable when storage is unavailable.
      }
    },
  );
  openRuns.set(pageKey, state);
  return state;
}

export function notesAtom(pageKey: string): PrimitiveAtom<FoldNote[]> {
  let state = notes.get(pageKey);
  if (!state) {
    state = atom<FoldNote[]>([]);
    notes.set(pageKey, state);
  }
  return state;
}

export function clearNotesIn(
  store: ReturnType<typeof createStore>,
  pageKey: string,
  paragraphOf: string,
): void {
  store.set(notesAtom(pageKey), (current) =>
    current.filter((note) => note.paragraphOf !== paragraphOf),
  );
}
