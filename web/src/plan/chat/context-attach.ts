import { atom } from "jotai/vanilla";

import type { AttachmentDraft } from "@/plan/attachments/types";
import { repoRel } from "@/plan/files/address";
import type { Store } from "@/state/store";
import { draftAtom, MAX_ATTACHMENTS } from "./drafts";
import { sessionAtom } from "./model";
import { isActiveSession } from "./session-home";
import { contextDocumentAtom } from "./viewed-document";

/**
 * The last viewed document the chat was given, and the session that was given it. `session` is
 * null for a chat the send is starting, whose id the snapshot has not reported yet;
 * `bindAttachedSession` fills it in when it does. A different session never matches, so a new
 * chat starts with nothing attached.
 */
export const lastAttachedAtom = atom<{ path: string; session: string | null } | null>(null);

/** How long a send waits for the viewed document before it goes without it. */
const FETCH_LIMIT_MS = 4_000;

// A concluded session takes no more messages: the next send starts a chat, so it counts as no
// session. Otherwise a chat started from a concluded panel would be recorded against the dead id.
const sessionIdAtom = atom((get) => {
  const session = get(sessionAtom);
  return isActiveSession(session) ? session.id : null;
});

/** What a send should do about the viewed document; null when it adds and counts nothing. */
export type ContextWish = {
  path: string;
  /** The draft already holds a source attachment for this path, so none is added. */
  held: boolean;
};

function holdsSource(drafts: AttachmentDraft[], path: string): boolean {
  return drafts.some((item) => item.kind === "source" && repoRel(item.path) === repoRel(path));
}

function alreadyGiven(store: Store, path: string): boolean {
  const last = store.get(lastAttachedAtom);
  const session = store.get(sessionIdAtom);
  // With no session this send starts a chat, which has been given nothing.
  if (last === null || last.path !== path || session === null) return false;
  return last.session === null || last.session === session;
}

/**
 * The server refuses an attachment path holding whitespace or `#` (its header cannot carry them),
 * and the refusal fails the whole message. A viewed document the user never added must not cost
 * the send, so such a page is never offered.
 */
const REFUSED_PATH = /[\p{White_Space}#]/u;

/**
 * The viewed document a message sent now should carry: the page's source when it has one, the
 * server accepts its path, the user has not dismissed it, and the chat was not given it already.
 */
export function contextWish(store: Store): ContextWish | null {
  const { path } = store.get(contextDocumentAtom);
  if (path === null || REFUSED_PATH.test(path) || alreadyGiven(store, path)) return null;
  const drafts = store.get(draftAtom).attachments;
  const held = holdsSource(drafts, path);
  if (!held && drafts.length >= MAX_ATTACHMENTS) return null;
  return { path, held };
}

/** Records that the chat now has `path`. Call only once the message was accepted. */
export function recordGiven(store: Store, path: string): void {
  store.set(lastAttachedAtom, { path, session: store.get(sessionIdAtom) });
}

/**
 * Forgets what the chat was given. A queued message that is withdrawn or returned may have been
 * the one that carried the viewed document, so the next send on that page attaches it again: a
 * redundant attachment is harmless and a missing one is not.
 */
export function forgetGiven(store: Store): void {
  store.set(lastAttachedAtom, null);
}

/** Resolves to `work`'s value, or null once `ms` pass first. */
export async function withinLimit<T>(work: Promise<T>, ms = FETCH_LIMIT_MS): Promise<T | null> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const late = new Promise<null>((resolve) => {
    timer = setTimeout(() => resolve(null), ms);
  });
  try {
    return await Promise.race([work, late]);
  } finally {
    clearTimeout(timer);
  }
}

/** Ties a record made while its chat was starting to that chat's id. Returns the unsubscribe. */
export function bindAttachedSession(store: Store): () => void {
  return store.sub(sessionIdAtom, () => {
    const session = store.get(sessionIdAtom);
    const last = store.get(lastAttachedAtom);
    if (session !== null && last !== null && last.session === null) {
      store.set(lastAttachedAtom, { ...last, session });
    }
  });
}
