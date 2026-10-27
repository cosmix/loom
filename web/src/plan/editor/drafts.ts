import { serverInfoAtom } from "@/plan/state/atoms";
import { store } from "@/state/store";

export type SaveDraftResult = "saved" | "too_large" | "storage_full" | "unavailable";
export type Draft = { openedHash: string; text: string; savedAt: number };

type StoredDraft = Draft & { projectKey: string; path: string };
type Entry = { key: string; draft: StoredDraft; bytes: number };

const PREFIX = "loom-plan-draft:";
const MAX_DRAFT = 1024 * 1024;
const MAX_TOTAL = 2 * MAX_DRAFT;
const encoder = new TextEncoder();

function projectKey(): string | null {
  return store.get(serverInfoAtom)?.project_key || null;
}

function keyFor(project: string, path: string, hash: string): string {
  return `${PREFIX}${project}:${path}:${hash}`;
}

function readEntries(storage: Storage): Entry[] {
  const entries: Entry[] = [];
  for (let i = 0; i < storage.length; i++) {
    const key = storage.key(i);
    if (!key?.startsWith(PREFIX)) continue;
    const raw = storage.getItem(key);
    if (raw === null) continue;
    let value: unknown;
    try {
      value = JSON.parse(raw);
    } catch {
      continue;
    }
    if (!value || typeof value !== "object") continue;
    const draft = value as Partial<StoredDraft>;
    if (
      typeof draft.projectKey !== "string" ||
      typeof draft.path !== "string" ||
      typeof draft.openedHash !== "string" ||
      typeof draft.text !== "string" ||
      typeof draft.savedAt !== "number" ||
      !Number.isFinite(draft.savedAt) ||
      key !== keyFor(draft.projectKey, draft.path, draft.openedHash)
    )
      continue;
    entries.push({ key, draft: draft as StoredDraft, bytes: encoder.encode(draft.text).length });
  }
  return entries.sort((a, b) => a.draft.savedAt - b.draft.savedAt);
}

export function loadDraft(path: string, openedHash: string): string | null {
  return findDrafts(path).find((draft) => draft.openedHash === openedHash)?.text ?? null;
}

export function findDrafts(path: string): Draft[] {
  try {
    const project = projectKey();
    if (!project) return [];
    return readEntries(localStorage)
      .filter(({ draft }) => draft.projectKey === project && draft.path === path)
      .reverse()
      .map(({ draft: { openedHash, text, savedAt } }) => ({ openedHash, text, savedAt }));
  } catch {
    return [];
  }
}

export function clearDrafts(path: string): void {
  try {
    const project = projectKey();
    if (!project) return;
    const storage = localStorage;
    for (const { key, draft } of readEntries(storage)) {
      if (draft.projectKey === project && draft.path === path) storage.removeItem(key);
    }
  } catch {
    // Storage may be blocked by the browser.
  }
}

export function saveDraft(path: string, openedHash: string, text: string): SaveDraftResult {
  const bytes = encoder.encode(text).length;
  if (bytes > MAX_DRAFT) return "too_large";
  try {
    const project = projectKey();
    if (!project) return "unavailable";
    const storage = localStorage;
    const key = keyFor(project, path, openedHash);
    const entries = readEntries(storage).filter((entry) => entry.key !== key);
    let total = entries.reduce((sum, entry) => sum + entry.bytes, bytes);
    while (total > MAX_TOTAL && entries.length) {
      const oldest = entries.shift()!;
      storage.removeItem(oldest.key);
      total -= oldest.bytes;
    }
    const value = JSON.stringify({
      projectKey: project,
      path,
      openedHash,
      text,
      savedAt: Date.now(),
    });
    while (true) {
      try {
        storage.setItem(key, value);
        return "saved";
      } catch (error) {
        // DOMException is not an Error subclass in every runtime (jsdom), so match by name.
        if ((error as { name?: unknown } | null)?.name !== "QuotaExceededError")
          return "unavailable";
        const oldest = entries.shift();
        if (!oldest) return "storage_full";
        storage.removeItem(oldest.key);
      }
    }
  } catch {
    return "unavailable";
  }
}
