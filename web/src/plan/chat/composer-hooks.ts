import { atom, useAtom, useAtomValue, useStore } from "jotai";
import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ClipboardEvent,
  type DragEvent,
} from "react";

import type { AnyAttachmentInput, Attachment, SessionView } from "@/plan/api/schema-chat";
import type { TreeEntry } from "@/plan/api/schema-files";
import type { CommandsView } from "@/plan/api/schema-session-commands";
import { MAX_ATTACHMENT_BYTES, type AttachmentDraft } from "@/plan/attachments";
import { docAttachment } from "@/plan/attachments/doc-attachment";
import { attachFiles, uploadStatusAtom } from "@/plan/attachments/intake";
import { fetchDoc } from "@/plan/files/api";
import { treeAtom } from "@/plan/files/atoms";
import { hashesAtom } from "@/plan/review/live";
import { canAuthorAtom, isHolderAtom } from "@/plan/state/atoms";
import { commandsAtom, loadingView } from "@/plan/state/commands-atom";
import type { Store } from "@/state/store";
import { sendMessage, startChat, toAttachmentInput } from "./commands";
import { contextWish, recordGiven, withinLimit } from "./context-attach";
import { draftAtom, MAX_ATTACHMENTS } from "./drafts";
import { messageProblem } from "./message-problem";
import { sessionAtom, unconfirmedSendsAtom, type UnconfirmedSend } from "./model";
import { holdNoteSeenAtom } from "./prefs";
import { isActiveSession, sessionHereAtom, startOwnerAtom } from "./session-home";
import { revealChat } from "./sheet-state";

/** Set by the `c` shortcut after it opens the panel; the mounted composer focuses on it. */
export const focusComposerAtom = atom(false);

/**
 * The id of the concluded session the user chose to write after (a clicked prompt). While it
 * matches the shown session the composer replaces New chat, and sending from it starts a new chat.
 * Keyed by session id, so it ends with that session and never reaches the next chat's end; it is
 * not persisted. The draft's words play no part: a stale or returned draft leaves New chat alone.
 */
export const composeAfterAtom = atom<string | null>(null);

const TYPING_TARGET =
  "input, textarea, select, [contenteditable]:not([contenteditable=false]), [role=textbox]";

/**
 * Registers the `c` key. It reveals the chat first (`revealChat`: a closed sheet opens, a collapsed side
 * panel expands), then asks the composer to focus. Call it from a component that stays mounted while the panel is hidden.
 * With `enabled` false (a screen that mounts no panel) the key is left alone: nothing there
 * would clear the requests it sets, so the sheet would open unasked on the next screen.
 */
export function useComposerShortcut(enabled = true): void {
  const store = useStore();
  useEffect(() => {
    if (!enabled) return;
    const onKey = (event: KeyboardEvent) => {
      if (
        event.defaultPrevented ||
        event.key !== "c" ||
        event.ctrlKey ||
        event.altKey ||
        event.metaKey ||
        event.isComposing
      )
        return;
      if (event.target instanceof Element && event.target.closest(TYPING_TARGET)) return;
      // A session that belongs elsewhere leaves the panel with no composer to focus.
      if (!store.get(sessionHereAtom)) return;
      event.preventDefault();
      revealChat(store);
      store.set(focusComposerAtom, true);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [store, enabled]);
}

/** Clears a focus request once the composer had its chance to take it (child effects run first). */
export function useClearFocusRequest(): void {
  const store = useStore();
  const requested = useAtomValue(focusComposerAtom);
  useEffect(() => {
    if (requested) store.set(focusComposerAtom, false);
  }, [requested, store]);
}

export function useComposerBox(text: string) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const requested = useAtomValue(focusComposerAtom);
  useEffect(() => {
    if (requested) ref.current?.focus();
  }, [requested]);
  useLayoutEffect(() => {
    const box = ref.current;
    if (!box) return;
    box.style.height = "auto";
    const line = Number.parseFloat(getComputedStyle(box).lineHeight) || 21;
    box.style.height = `${Math.min(line * 8 + 18, Math.max(line + 18, box.scrollHeight + 2))}px`;
  }, [text]);
  return ref;
}

/** True for the render session in which this browser first held authoring. */
export function useFirstHoldNote(): boolean {
  const holder = useAtomValue(isHolderAtom);
  const [seen, setSeen] = useAtom(holdNoteSeenAtom);
  const [shown, setShown] = useState(false);
  useEffect(() => {
    if (!holder || seen) return;
    setSeen(true);
    setShown(true);
  }, [holder, seen, setSeen]);
  return holder && shown;
}

/** The viewed document as an attachment; null when it cannot be fetched, and the send goes without. */
async function fetchContext(path: string): Promise<AttachmentDraft | null> {
  const result = await withinLimit(fetchDoc(path));
  return result?.ok ? docAttachment(result.value) : null;
}

/** The gate's view of an input: the server computes the lock and the change flag, not the browser. */
function asDelivered(input: AnyAttachmentInput): Attachment {
  const none = { anchor: null, lines: null, stage: null, summary_sentence: null, rests_on: [] };
  const flags = { locked: null, changed_since_selected: false };
  // A file travels by id and carries no quote, so only its header line counts toward the limit.
  if (input.kind === "file") return { kind: "file", path: "", quote: "", ...none, ...flags };
  const { selected_hash: _hash, ...rest } = input;
  return { ...rest, ...flags };
}

/**
 * The command list a send is checked against. A send with no active session starts a chat, which
 * the server checks against its loading table (the built-in rows alone), so the rows a concluded
 * session's list added must not decide it.
 */
export function commandsForSend(session: SessionView | null, commands: CommandsView): CommandsView {
  return isActiveSession(session) ? commands : loadingView(commands);
}

const problemWith = (store: Store, text: string, inputs: AnyAttachmentInput[]) =>
  messageProblem(
    text,
    inputs.map(asDelivered),
    commandsForSend(store.get(sessionAtom), store.get(commandsAtom)),
  );

type Outgoing = { text: string; attachments: AnyAttachmentInput[]; withViewed: boolean };

/**
 * The server refuses a quote over its limit and the whole message with it. `quoteLines` leaves a
 * document uncut when its last line already looks like the cut marker, so a viewed document can
 * still arrive over the limit here.
 */
function quoteFits(viewed: AttachmentDraft): boolean {
  return new TextEncoder().encode(viewed.text).length <= MAX_ATTACHMENT_BYTES;
}

/**
 * What a send carries, read after the viewed document was fetched: the draft is what the user
 * sees at the moment of sending, and the composer's gate ran before the wait. Null means send
 * nothing: the draft no longer passes (the user cleared the box during the wait), and it stays
 * as it is. If only the document makes the message unsendable (over the size limit, or a quote the
 * server would refuse), the message goes without it, because an attachment the user never added
 * must not cost them the send.
 * A passage the user attached stays first and the viewed document follows it; with no passage the
 * document is first, which is the attachment the server's guidance reads.
 * Synchronous on purpose: a send with nothing to fetch goes out in the press's own tick.
 */
function outgoing(
  store: Store,
  retry: UnconfirmedSend | undefined,
  viewed: AttachmentDraft | null,
): Outgoing | null {
  // A resend carries what its first send carried.
  if (retry) return { text: retry.text, attachments: retry.attachments ?? [], withViewed: false };
  const { text, attachments } = store.get(draftAtom);
  const drafted = attachments.map(toAttachmentInput);
  if (problemWith(store, text, drafted) !== null) return null;
  const withViewed = viewed && quoteFits(viewed) ? [...drafted, toAttachmentInput(viewed)] : null;
  if (withViewed && problemWith(store, text, withViewed) === null) {
    return { text, attachments: withViewed, withViewed: true };
  }
  return { text, attachments: drafted, withViewed: false };
}

/** Takes the sent words and attachments out of the draft; whatever the user typed meanwhile stays. */
function clearSent(store: Store, sent: Outgoing): void {
  store.set(draftAtom, (current) => ({
    text: current.text === sent.text ? "" : current.text,
    attachments: current.attachments.filter(
      (item) =>
        !sent.attachments.some(
          (input) => JSON.stringify(toAttachmentInput(item)) === JSON.stringify(input),
        ),
    ),
  }));
}

/**
 * The stores with a send under way. The guard belongs to the store, not to the hook: collapsing
 * and reopening the panel during the viewed document's fetch mounts a new hook, which must not
 * start a second send.
 */
const sending = new WeakSet<Store>();

export function useSendDraft() {
  const store = useStore();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const send = async (resendId?: string) => {
    if (sending.has(store)) return;
    const retry = store.get(unconfirmedSendsAtom).find((item) => item.client_id === resendId);
    // A resend whose entry is gone (confirmed meanwhile) must not send the current draft instead.
    if (resendId !== undefined && !retry) return;
    // Held across the fetch below, so a second press of Send cannot start a second send.
    sending.add(store);
    setPending(true);
    setError(null);
    try {
      // Read at the press, as start-chat.tsx does: the screen can change during the fetch, and
      // the chat belongs to the feature or plan the user pressed Send on.
      const { feature, plan } = store.get(startOwnerAtom);
      const wish = retry ? null : contextWish(store);
      const viewed = wish && !wish.held ? await fetchContext(wish.path) : null;
      const sent = outgoing(store, retry, viewed);
      if (sent === null) return;
      // A concluded session takes no messages: sending from its panel starts a new chat.
      const result = isActiveSession(store.get(sessionAtom))
        ? await sendMessage(store, sent.text, sent.attachments)
        : await startChat(store, null, sent.text, sent.attachments, feature, plan);
      if (!result.ok) throw new Error(result.error ?? "Command refused");
      if (wish && (wish.held || sent.withViewed)) recordGiven(store, wish.path);
      if (resendId) {
        store.set(unconfirmedSendsAtom, (items) =>
          items.filter((item) => item.client_id !== resendId),
        );
      } else {
        clearSent(store, sent);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      sending.delete(store);
      setPending(false);
    }
  };
  return { pending, error, send };
}

function flattened(entries: TreeEntry[]): TreeEntry[] {
  return entries.flatMap((entry) => [entry, ...flattened(entry.children)]);
}

function attachmentLock(path: string, entries: TreeEntry[]): Attachment["locked"] {
  if (path.startsWith("doc/loom/knowledge/")) return "knowledge";
  const entry = entries.find((item) => item.path === path);
  if (!entry?.locked) return null;
  if (entry.kind === "brief" || entry.kind === "brief_folder") return "brief";
  if (entry.state === "running" || entry.state === "retired") return entry.state;
  return "done";
}

export function useDeliveredAttachments(): Attachment[] {
  const draft = useAtomValue(draftAtom);
  const hashes = useAtomValue(hashesAtom);
  const tree = useAtomValue(treeAtom);
  const entries = tree ? flattened(Object.values(tree).flat()) : [];
  return draft.attachments.map((item) => {
    if (item.kind === "file") return asDelivered(toAttachmentInput(item));
    const input = toAttachmentInput(item);
    return {
      ...input,
      locked: attachmentLock(input.path, entries),
      changed_since_selected:
        item.hash !== null &&
        hashes?.docs[input.path] !== undefined &&
        hashes.docs[input.path] !== item.hash,
    };
  });
}

const hasFiles = (event: DragEvent) => Array.from(event.dataTransfer.types).includes("Files");

/**
 * Files dropped on the composer and images pasted into it become uploaded chips. Spread the
 * handlers on the composer box and draw the drop target from `dragging`. An upload counts toward
 * the attachment cap, so only as many files as fit are sent; the rest are named in a refusal.
 */
export function useFileIntake() {
  const store = useStore();
  const [dragging, setDragging] = useState(false);
  const take = (files: File[]) => {
    if (files.length === 0) return;
    if (!store.get(canAuthorAtom)) {
      store.set(uploadStatusAtom, (status) => ({
        ...status,
        problems: ["Attaching a file needs authoring, which another browser holds."],
      }));
      return;
    }
    const room = Math.max(0, MAX_ATTACHMENTS - store.get(draftAtom).attachments.length);
    void attachFiles(store, files, room);
  };
  return {
    dragging,
    onDragOver: (event: DragEvent) => {
      if (!hasFiles(event)) return;
      event.preventDefault();
      setDragging(true);
    },
    onDragLeave: (event: DragEvent) => {
      // Entering a child of the box fires a leave on the box; only leaving the box ends the drag.
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDragging(false);
    },
    onDrop: (event: DragEvent) => {
      if (!hasFiles(event)) return;
      event.preventDefault();
      setDragging(false);
      take(Array.from(event.dataTransfer.files));
    },
    onPaste: (event: ClipboardEvent) => {
      const files = Array.from(event.clipboardData.files);
      if (files.length === 0) return;
      // Copying cells or a rich selection puts text and a picture on the clipboard
      // together; the text is what the person meant, so only a files-only paste is taken.
      if (event.clipboardData.getData("text/plain") !== "") return;
      event.preventDefault();
      take(files);
    },
  };
}
