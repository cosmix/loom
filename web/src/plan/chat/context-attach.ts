import { atom } from "jotai/vanilla";

import type { ChatAttachment } from "@/plan/attachments/types";
import { repoRel } from "@/plan/files/address";
import { projectAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";
import { draftAtom, MAX_ATTACHMENTS } from "./drafts";
import { sessionAtom } from "./model";
import { isActiveSession, screenAtom } from "./session-home";
import { contextDocumentAtom, type ViewedKind } from "./viewed-document";

/**
 * The last viewed document the chat was given, and the session that was given it. `session` is
 * null for a chat the send is starting, whose id the snapshot has not reported yet;
 * `bindAttachedSession` fills it in when it does. A different session never matches, so a new
 * chat starts with nothing attached.
 */
export const lastAttachedAtom = atom<{ path: string; session: string | null } | null>(null);

/** How long a send waits for the viewed document before it goes without it. */
const FETCH_LIMIT_MS = 4_000;

/** How long a send waits for the project that names the viewed document's source file. */
export const NAMING_LIMIT_MS = 1_500;

/**
 * How long a send keeps waiting once the project is here and the viewed document still has no
 * path: `ScreenSync` rewrites the viewed document in a layout effect after React renders the
 * project, so the path can follow the project by a frame. Past this the page has no source file.
 */
const PROJECT_SETTLE_MS = 100;

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

function holdsSource(drafts: ChatAttachment[], path: string): boolean {
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

/** Which document on which screen a send was pressed on; the path is what the wait may fill in. */
export type PressedContext = {
  kind: ViewedKind;
  name: string;
  feature: string | null;
  plan: string | null;
};

export function pressedContext(store: Store): PressedContext {
  const { kind, name } = store.get(contextDocumentAtom);
  const { feature = null, plan = null } = store.get(screenAtom) ?? {};
  return { kind, name, feature, plan };
}

function samePage(a: PressedContext, b: PressedContext): boolean {
  return a.kind === b.kind && a.name === b.name && a.feature === b.feature && a.plan === b.plan;
}

/**
 * The viewed document a message sent now should carry: the page's source when it has one, the
 * server accepts its path, the user has not dismissed it, and the chat was not given it already.
 * With `pressed`, only that page's document counts: a send that waited for the path while the
 * user moved on carries nothing, because the chat belongs to the screen the press was made on.
 */
export function contextWish(store: Store, pressed?: PressedContext): ContextWish | null {
  if (pressed && !samePage(pressed, pressedContext(store))) return null;
  const { path } = store.get(contextDocumentAtom);
  if (path === null || REFUSED_PATH.test(path) || alreadyGiven(store, path)) return null;
  const drafts = store.get(draftAtom).attachments;
  const held = holdsSource(drafts, path);
  if (!held && drafts.length >= MAX_ATTACHMENTS) return null;
  return { path, held };
}

// The chip shows a page by its route, which needs no checklist, while its source path comes from
// the checklist in the project frame. Until that frame arrives the chip is there and no path is.
function lacksPath(store: Store): boolean {
  const { kind, path } = store.get(contextDocumentAtom);
  return kind !== null && path === null;
}

/**
 * Whether the viewed document is named by the page's route and its source path is still to come
 * with the project frame. A page that has no path once the project is here never gets one, so it
 * is not waited for. A send checks this first and only then awaits `settledContext`: a send with
 * nothing to wait for stays synchronous up to the socket write.
 */
export function awaitsContextPath(store: Store): boolean {
  return lacksPath(store) && store.get(projectAtom) === null;
}

/**
 * Resolves once the viewed document has a source path, the page has no document (the route moved
 * on, or the user dismissed it), the project arrived and the page still has no path
 * (`PROJECT_SETTLE_MS` after it), or `NAMING_LIMIT_MS` pass. A send awaits it before
 * `contextWish`, so a message sent while the context chip shows carries that document even when
 * the project frame is still on its way, and one for a page with no source file does not sit out
 * the whole limit.
 */
export async function settledContext(store: Store): Promise<void> {
  let settle: ReturnType<typeof setTimeout> | undefined;
  const stops: Array<() => void> = [];
  const named = new Promise<void>((resolve) => {
    stops.push(
      store.sub(contextDocumentAtom, () => {
        if (!lacksPath(store)) resolve();
      }),
      store.sub(projectAtom, () => {
        if (store.get(projectAtom) !== null) settle ??= setTimeout(resolve, PROJECT_SETTLE_MS);
      }),
    );
  });
  try {
    await withinLimit(named, NAMING_LIMIT_MS);
  } finally {
    stops.forEach((stop) => stop());
    clearTimeout(settle);
  }
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
