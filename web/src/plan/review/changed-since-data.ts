import { atom, useAtomValue, useStore } from "jotai";
import { useEffect, useRef, useState } from "react";

import type { ApiResult } from "@/plan/api/client";
import { fetchDoc } from "@/plan/files/api";
import { planSocketAtom, serverInfoAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";

import { getAtCommit, getCompare, type CompareResponse } from "./api";
import type { Change, Comparison } from "./changed-since-parts";
import { changedSince, checkpointAtom, checkpointGenerationAtom, markSeen } from "./checkpoint";
import { hashesAtom, loadHashes } from "./live";
import {
  TOTAL_LIMIT,
  fitsDocLimit,
  recordSeen,
  seenBaseline,
  seenTextsAtom,
  type SeenRead,
} from "./seen-text";

/** One open diff: the document and the ref it compares with, if any. */
export type Selection = { path: string; ref: Comparison | null };
/** Where a diff's before side came from: what this browser saw, or a commit. */
export type Baseline = { kind: "seen"; text: string } | { kind: "commit"; commit: string };
type DiffLoad =
  | { before: string | null; after: string | null; baseline: Baseline }
  | { error: string }
  | null;
type CompareLoad = { value?: CompareResponse; error?: string; loading: boolean };

// One computation shared by every tree row, key chip and the activity popover's row.
const changesAtom = atom((get) => {
  const checkpoint = get(checkpointAtom);
  const current = get(hashesAtom);
  return checkpoint && current ? changedSince(checkpoint, current.docs) : [];
});

export function useChangedSince(): Change[] {
  const store = useStore();
  const current = useAtomValue(hashesAtom);
  const projectKey = useAtomValue(serverInfoAtom)?.project_key;
  useEffect(() => {
    if (projectKey && current && store.get(checkpointAtom) === null) {
      store.set(checkpointAtom, { commit: current.head, hashes: { ...current.docs } });
    }
  }, [store, current, projectKey]);
  return useAtomValue(changesAtom);
}

export function useCompare(commit: string | null) {
  const socket = useAtomValue(planSocketAtom);
  const hashes = useAtomValue(hashesAtom);
  const [attempt, setAttempt] = useState(0);
  const [load, setLoad] = useState<CompareLoad>({ loading: true });
  useEffect(() => socket?.onResync(() => setAttempt((n) => n + 1)), [socket]);
  useEffect(() => {
    let active = true;
    if (commit === null) {
      setLoad({ loading: false });
      return;
    }
    setLoad({ loading: true });
    void getCompare(commit).then((result) => {
      if (!active) return;
      setLoad(
        result.ok
          ? { loading: false, value: result.value }
          : { loading: false, error: result.message },
      );
    });
    return () => {
      active = false;
    };
  }, [commit, hashes, attempt]);
  return { ...load, retry: () => setAttempt((n) => n + 1) };
}

/** What a pending write was cut from: the project, and the checkpoint's generation in it. */
type Guard = { projectKey: string | undefined; generation: number };

function guardOf(store: Store): Guard {
  return {
    projectKey: store.get(serverInfoAtom)?.project_key,
    generation: store.get(checkpointGenerationAtom),
  };
}

const sameGuard = (a: Guard, b: Guard) =>
  a.projectKey === b.projectKey && a.generation === b.generation;

/** A ref compares with that ref; otherwise the seen text while intact, else the checkpoint commit. */
function baselineFor(store: Store, path: string, ref: Comparison | null): Baseline | null {
  if (ref) return { kind: "commit", commit: ref.commit };
  const checkpoint = store.get(checkpointAtom);
  const text = seenBaseline(store.get(seenTextsAtom), checkpoint, path);
  if (text !== null) return { kind: "seen", text };
  return checkpoint?.commit ? { kind: "commit", commit: checkpoint.commit } : null;
}

async function beforeText(baseline: Baseline, path: string): Promise<ApiResult<string | null>> {
  if (baseline.kind === "seen") return { ok: true, value: baseline.text };
  const result = await getAtCommit(baseline.commit, path);
  return result.ok ? { ok: true, value: result.value.text } : result;
}

/** `seen` is set only when the diff shows a complete current text (null: the document is gone). */
async function readDiff(
  store: Store,
  path: string,
  ref: Comparison | null,
): Promise<{ load: DiffLoad; seen?: SeenRead }> {
  const baseline = baselineFor(store, path, ref);
  if (!baseline) return { load: { error: "No earlier text of this document is available." } };
  const [before, after] = await Promise.all([beforeText(baseline, path), fetchDoc(path)]);
  if (!before.ok) return { load: { error: before.message } };
  if (!after.ok && after.status !== 404) return { load: { error: after.message } };
  if (after.ok && (after.value.too_large || after.value.truncated)) {
    return { load: { error: "This document is too large to show a complete diff." } };
  }
  const seen = after.ok ? { hash: after.value.hash, text: after.value.text } : null;
  return { load: { before: before.value, after: seen?.text ?? null, baseline }, seen };
}

/** Marks one document seen: its hash in the checkpoint and its text in the seen-text store. */
function acknowledge(store: Store, path: string, seen: SeenRead): void {
  markSeen(store, path, seen?.hash ?? null);
  recordSeen(store, { [path]: seen });
}

/**
 * Loads the selected diff, again whenever the document's current hash moves. A checkpoint-mode
 * diff that loads marks the document seen, unless the checkpoint was replaced (Mark all seen)
 * while it loaded. The shown sides stay as fetched when that acknowledgment moves the baseline.
 * The effect's cleanup drops a response superseded by a newer fetch of the same document.
 */
export function useSelectedDiff({ path, ref }: Selection) {
  const store = useStore();
  const hash = useAtomValue(hashesAtom)?.docs[path];
  const [load, setLoad] = useState<DiffLoad>(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let active = true;
    const started = guardOf(store);
    void readDiff(store, path, ref).then(({ load: next, seen }) => {
      const now = guardOf(store);
      if (!active || now.projectKey !== started.projectKey) return;
      setLoad(next);
      if (seen !== undefined && ref === null && sameGuard(now, started)) {
        acknowledge(store, path, seen);
      }
    });
    return () => {
      active = false;
    };
    // `hash` only triggers the refetch: the live document moved.
  }, [store, path, ref, hash, attempt]);
  return { load, retry: () => setAttempt((n) => n + 1) };
}

async function readSeen(path: string): Promise<SeenRead> {
  const result = await fetchDoc(path);
  if (!result.ok || result.value.too_large || result.value.truncated) return null;
  return { hash: result.value.hash, text: result.value.text };
}

/** Most `/api/doc` reads of Mark all seen in flight at once. */
const READ_CONCURRENCY = 4;

/**
 * Reads the listed documents with at most `READ_CONCURRENCY` requests in flight. Once the storable
 * texts read so far pass the store's total cap no further read is issued: the store would evict
 * them. A text over the per-document cap is not stored, so it does not count. No further read is
 * issued once `current` says the checkpoint moved. The size counts UTF-16 bytes of path and text,
 * without the JSON overhead.
 */
async function readAll(
  paths: string[],
  current: () => boolean,
): Promise<Array<readonly [string, SeenRead]>> {
  const reads: Array<readonly [string, SeenRead]> = [];
  let next = 0;
  let bytes = 0;
  const worker = async () => {
    while (next < paths.length && bytes <= TOTAL_LIMIT && current()) {
      const path = paths[next++]!;
      const read = await readSeen(path);
      if (read && fitsDocLimit(path, read)) bytes += (path.length + read.text.length) * 2;
      reads.push([path, read]);
    }
  };
  await Promise.all(Array.from({ length: Math.min(READ_CONCURRENCY, paths.length) }, worker));
  return reads;
}

/**
 * Stores the current text of each listed document, unless the checkpoint was replaced meanwhile.
 * `markSeen` leaves the generation, so a document acknowledged while its read was pending is also
 * left alone: its entry was seen after the batch started, or its checkpoint hash is no longer the
 * one the read was taken at (for a failed read, the hash when the batch started). Either way a
 * slow read never replaces or deletes the newer acknowledgment.
 */
async function storeSeenTexts(store: Store, paths: string[]): Promise<void> {
  const started = guardOf(store);
  const startedAt = Date.now();
  const hashesAtStart = { ...store.get(checkpointAtom)?.hashes };
  const reads = await readAll(paths, () => sameGuard(guardOf(store), started));
  if (!sameGuard(guardOf(store), started)) return;
  const entries = store.get(seenTextsAtom);
  const hashes = store.get(checkpointAtom)?.hashes ?? {};
  const next: Record<string, SeenRead> = {};
  for (const [path, read] of reads) {
    const entry = Object.hasOwn(entries, path) ? entries[path] : undefined;
    if (entry && entry.seen >= startedAt) continue;
    if (hashes[path] === (read ? read.hash : hashesAtStart[path])) next[path] = read;
    // A read at another hash is skipped; an entry that no longer matches the checkpoint is dead.
    else if (read && entry && entry.hash !== hashes[path]) next[path] = null;
  }
  recordSeen(store, next);
}

type MarkState = { pending: boolean; error: string | null };
export type MarkAll = MarkState & { markAll: (paths: string[]) => void };

/**
 * Stores what the server holds when the button is pressed. `hashesAtom` reloads on load, reconnect
 * and `doc_changed`, and a commit sends none of them, so its `head` can predate the commit HEAD
 * points to now; the checkpoint must hold that commit. The load goes through the serial queue of
 * `loadHashes`, which also replaces `hashesAtom` with the response the checkpoint is cut from; a
 * `doc_changed` reload that starts meanwhile queues behind it, so an older response never lands
 * after a newer one. The texts of the listed documents follow in the background: one that fails
 * keeps no text, so its next diff starts from the new checkpoint commit.
 */
export function useMarkAllSeen(): MarkAll {
  const store = useStore();
  const [state, setState] = useState<MarkState>({ pending: false, error: null });
  const mounted = useRef(true);
  const busy = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const markAll = (paths: string[]) => {
    if (busy.current) return;
    busy.current = true;
    setState({ pending: true, error: null });
    void loadHashes(store).then((result) => {
      busy.current = false;
      if (!mounted.current) return;
      if (result.ok) {
        store.set(checkpointAtom, { commit: result.value.head, hashes: { ...result.value.docs } });
        void storeSeenTexts(store, paths);
      }
      setState({ pending: false, error: result.ok ? null : result.message });
    });
  };
  return { ...state, markAll };
}
