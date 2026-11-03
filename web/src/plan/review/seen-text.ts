import { z } from "@/api/zod";
import { projectPref } from "@/plan/chat/prefs";
import { safeStorage, storageKey } from "@/plan/chat/storage";
import { serverInfoAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";

import type { Checkpoint } from "./checkpoint";

/** What this browser saw of a document: the text, the hash it was read at, and when. */
export type SeenEntry = { hash: string; text: string; seen: number };
export type SeenTexts = Record<string, SeenEntry>;
/** A document as read for the store; null when it is gone or could not be read. */
export type SeenRead = { hash: string; text: string } | null;

/**
 * Caps in bytes of stored JSON: browser storage holds UTF-16, so each code unit costs two bytes.
 * The total cap covers the seen texts of every project on this origin, since the storage quota is
 * per origin and every repository's planner serves on the same one.
 */
export const DOC_LIMIT = 256 * 1024;
export const TOTAL_LIMIT = 2 * 1024 * 1024;

const NAME = "seen-text";
/** `"path":{...},` as it sits inside the stored object. */
const entryBytes = (path: string, entry: SeenEntry) =>
  (JSON.stringify(path).length + JSON.stringify(entry).length + 2) * 2;
/** The key itself and the object's braces. */
const keyBytes = (key: string) => (key.length + 2) * 2;

const seenTextsSchema = z.record(
  z.string(),
  z.object({ hash: z.string(), text: z.string(), seen: z.number() }),
);

/**
 * The seen texts of the current project, beside its checkpoint in browser storage
 * (`storageKey(project, "seen-text")`). `projectPref` reads and writes through `safeStorage`, so a
 * blocked or unreadable store reads as empty and every diff falls back to the checkpoint commit.
 */
export const seenTextsAtom = projectPref<SeenTexts>(NAME, {}, seenTextsSchema);

/** The stored text a diff of `path` starts from: only while its hash is the checkpoint's. */
export function seenBaseline(
  entries: SeenTexts,
  checkpoint: Checkpoint | null,
  path: string,
): string | null {
  const entry = Object.hasOwn(entries, path) ? entries[path] : undefined;
  return entry && checkpoint?.hashes[path] === entry.hash ? entry.text : null;
}

// The key format stays in `storageKey`: split its output around a marker no project key holds.
const MARKER = "\u0000";
const PATTERN = storageKey(MARKER, NAME);
const KEY_PREFIX = PATTERN.slice(0, PATTERN.indexOf(MARKER));
const KEY_SUFFIX = PATTERN.slice(PATTERN.indexOf(MARKER) + MARKER.length);

const isSeenTextKey = (key: string) =>
  key.length > KEY_PREFIX.length + KEY_SUFFIX.length &&
  key.startsWith(KEY_PREFIX) &&
  key.endsWith(KEY_SUFFIX);

/** Every other project's seen texts on this origin, by key; none when storage cannot be listed. */
function otherStores(ownKey: string): Array<[string, SeenTexts]> {
  const keys: string[] = [];
  try {
    for (let index = 0; index < localStorage.length; index++) {
      const key = localStorage.key(index);
      if (key !== null && key !== ownKey && isSeenTextKey(key)) keys.push(key);
    }
  } catch {
    return [];
  }
  return keys.map((key) => {
    const parsed = seenTextsSchema.safeParse(safeStorage.getItem<unknown>(key, null));
    return [key, parsed.success ? parsed.data : {}];
  });
}

/**
 * Drops the least recently seen entries across every store until all of them fit in the total
 * cap. Returns only the stores that lost entries.
 */
function fitTotal(stores: Map<string, SeenTexts>): Map<string, SeenTexts> {
  const slots = [...stores].flatMap(([key, entries]) =>
    Object.entries(entries).map(([path, entry]) => ({ key, path, entry })),
  );
  let total = 0;
  for (const key of stores.keys()) total += keyBytes(key);
  for (const { path, entry } of slots) total += entryBytes(path, entry);
  const changed = new Map<string, SeenTexts>();
  for (const { key, path, entry } of slots.sort((a, b) => a.entry.seen - b.entry.seen)) {
    if (total <= TOTAL_LIMIT) break;
    let kept = changed.get(key);
    if (!kept) {
      kept = { ...stores.get(key) };
      changed.set(key, kept);
    }
    delete kept[path];
    total -= entryBytes(path, entry);
  }
  return changed;
}

/** Rewrites another project's store after eviction; one left empty is removed. */
function writeOther(key: string, entries: SeenTexts): void {
  if (Object.keys(entries).length === 0) safeStorage.removeItem(key);
  else safeStorage.setItem(key, entries);
}

/**
 * Replaces one document's entry. A gone, unreadable or over-cap document keeps no text, so its
 * diff falls back to the checkpoint commit and an older text never pairs with a newer hash.
 */
function withSeen(entries: SeenTexts, path: string, read: SeenRead, now: number): SeenTexts {
  const entry = read && { hash: read.hash, text: read.text, seen: now };
  if (entry === null || entryBytes(path, entry) > DOC_LIMIT) {
    if (!Object.hasOwn(entries, path)) return entries;
    const rest = { ...entries };
    delete rest[path];
    return rest;
  }
  return { ...entries, [path]: entry };
}

/**
 * Stores what was seen of each document in one storage write. Eviction across every project's
 * store runs before the write, so this store never fills browser storage by itself (a quota error
 * would raise the global storage-full banner through `safeStorage`); other data on the origin can
 * still fill it. Before the project key is known the store lives in memory and only its own
 * entries count.
 */
export function recordSeen(store: Store, reads: Record<string, SeenRead>): void {
  const before = store.get(seenTextsAtom);
  const now = Date.now();
  let next = before;
  for (const [path, read] of Object.entries(reads)) next = withSeen(next, path, read, now);
  if (next === before) return;
  const projectKey = store.get(serverInfoAtom)?.project_key;
  const ownKey = projectKey == null ? "" : storageKey(projectKey, NAME);
  const stores = new Map(projectKey == null ? [] : otherStores(ownKey));
  stores.set(ownKey, next);
  const evicted = fitTotal(stores);
  for (const [key, entries] of evicted) if (key !== ownKey) writeOther(key, entries);
  store.set(seenTextsAtom, evicted.get(ownKey) ?? next);
}
