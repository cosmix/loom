import { atom, useAtom, useAtomValue, useStore } from "jotai";
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import type { Attachment } from "@/plan/api/schema-chat";
import type { TreeEntry } from "@/plan/api/schema-files";
import { treeAtom } from "@/plan/files/atoms";
import { hashesAtom } from "@/plan/review/live";
import { isHolderAtom } from "@/plan/state/atoms";
import { sendMessage, startChat, toAttachmentInput } from "./commands";
import { draftAtom } from "./drafts";
import { sessionAtom, unconfirmedSendsAtom } from "./model";
import { holdNoteSeenAtom, panelCollapsedAtom } from "./prefs";
import { chatSheetOpenAtom } from "./sheet-state";

/** Set by the `c` shortcut after it opens the panel; the mounted composer focuses on it. */
export const focusComposerAtom = atom(false);

const TYPING_TARGET =
  "input, textarea, select, [contenteditable]:not([contenteditable=false]), [role=textbox]";

/**
 * Registers the `c` key. It opens a collapsed panel or a closed sheet first, then asks the
 * composer to focus. Call it from a component that stays mounted while the panel is hidden.
 */
export function useComposerShortcut(): void {
  const store = useStore();
  useEffect(() => {
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
      event.preventDefault();
      store.set(panelCollapsedAtom, false);
      store.set(chatSheetOpenAtom, true);
      store.set(focusComposerAtom, true);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [store]);
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

export function useSendDraft() {
  const store = useStore();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inFlight = useRef(false);
  const send = async (resendId?: string) => {
    if (inFlight.current) return;
    const submitted = store.get(draftAtom);
    const retry = store.get(unconfirmedSendsAtom).find((item) => item.client_id === resendId);
    // A resend whose entry is gone (confirmed meanwhile) must not send the current draft instead.
    if (resendId !== undefined && !retry) return;
    const text = retry?.text ?? submitted.text;
    const attachments = retry?.attachments ?? submitted.attachments.map(toAttachmentInput);
    inFlight.current = true;
    setPending(true);
    setError(null);
    try {
      const result =
        store.get(sessionAtom) === null
          ? await startChat(store, null, text, attachments)
          : await sendMessage(store, text, attachments);
      if (!result.ok) throw new Error(result.error ?? "Command refused");
      if (resendId)
        store.set(unconfirmedSendsAtom, (items) =>
          items.filter((item) => item.client_id !== resendId),
        );
      // A resend sends an old entry, not the draft: whatever the user typed meanwhile stays.
      if (resendId === undefined) {
        store.set(draftAtom, (current) => ({
          text: current.text === text ? "" : current.text,
          attachments: current.attachments.filter(
            (item) =>
              !attachments.some(
                (sent) => JSON.stringify(toAttachmentInput(item)) === JSON.stringify(sent),
              ),
          ),
        }));
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      inFlight.current = false;
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
