import { useAtomValue, useStore } from "jotai/react";
import { useRef, useState, type JSX } from "react";

import type { Attachment, AttachmentInput, Item, Turn } from "@/plan/api/schema-chat";
import { sendMessage } from "@/plan/chat/commands";
import { composerBlock } from "@/plan/chat/composer-gate";
import {
  CONNECTION_LOST,
  RESEND_OFFLINE,
  resendingEntriesAtom,
  resendKey,
  resentItemsAtom,
  sessionAtom,
  turnsAtom,
  unconfirmedSendsAtom,
} from "@/plan/chat/model";
import { sameMessage } from "@/plan/chat/model-identity";
import { codexRunsAlone } from "@/plan/chat/pressure/steps";
import { pressureRunAtom } from "@/plan/chat/pressure/use-pressure";
import { Untrusted } from "@/plan/chat/visible-controls";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { canAuthorAtom, connectionAtom, serverInfoAtom, stoppingAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";

type UserItem = Extract<Item, { type: "user" }>;

/**
 * The server computes `changed_since_selected` as "a selected_hash was sent and differs from the
 * current hash" (plan_session/attachments/resolve.rs). The delivered attachment keeps no real
 * hash, so a flagged one resends a value no document hash can equal.
 * Revisit if the wire gains a field that carries the flag itself.
 */
const CHANGED_SINCE_SELECTED = "changed-since-selected";

/** The wire form of a delivered attachment: the lock flag is the server's to compute. */
function toInput({ locked: _locked, changed_since_selected: changed, ...rest }: Attachment) {
  const selected_hash = changed ? CHANGED_SINCE_SELECTED : null;
  return { ...rest, selected_hash } satisfies AttachmentInput;
}

/** Whether this message may offer Resend: may author, own message, current chat, composer open. */
function useMayResend(item: UserItem, turn: Turn): boolean {
  const canAuthor = useAtomValue(canAuthorAtom);
  const session = useAtomValue(sessionAtom);
  const stopping = useAtomValue(stoppingAtom);
  const codexAlone = codexRunsAlone(useAtomValue(pressureRunAtom));
  const readOnly = (useAtomValue(serverInfoAtom)?.read_only.length ?? 0) > 0;
  const resent = useAtomValue(resentItemsAtom).has(resendKey(turn, item.id));
  const inCurrentChat = useAtomValue(turnsAtom).some(
    (entry) => entry.id === turn.id && entry.started_at === turn.started_at,
  );
  // An ended chat takes no messages: its composer, when the user opens one, starts a new chat
  // and offers no Resend (ComposerArea), so there is nothing left to resend into.
  const blocked =
    composerBlock(session, { stopping, readOnly, codexAlone }) ?? session?.state === "ended";
  return canAuthor && item.origin === null && inCurrentChat && !blocked && !resent;
}

function setMember(store: Store, target: typeof resentItemsAtom, key: string, member: boolean) {
  store.set(target, (keys) => {
    const next = new Set(keys);
    if (member) next.add(key);
    else next.delete(key);
    return next;
  });
}

/**
 * A lost send keeps its own entry (same content and attachments); a send clears that entry.
 * The item counts as resent from the moment the resend is issued: if the connection drops before
 * the result, the server may have accepted it, and the unconfirmed entry then stands for the
 * message. Only an immediate refusal gives the control back.
 */
function useResend(item: UserItem, turn: Turn) {
  const store = useStore();
  const unconfirmed = useAtomValue(unconfirmedSendsAtom).find((send) => sameMessage(send, item));
  const inFlight = useRef(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const resend = async () => {
    if (inFlight.current) return;
    inFlight.current = true;
    setBusy(true);
    setError(null);
    const key = resendKey(turn, item.id);
    setMember(store, resentItemsAtom, key, true);
    if (unconfirmed) setMember(store, resendingEntriesAtom, unconfirmed.client_id, true);
    try {
      const result = await sendMessage(
        store,
        item.text,
        unconfirmed ? unconfirmed.attachments : item.attachments.map(toInput),
      );
      if (!result.ok) throw new Error(result.error ?? "Command refused");
      if (unconfirmed) {
        store.set(unconfirmedSendsAtom, (sends) =>
          sends.filter((send) => send.client_id !== unconfirmed.client_id),
        );
      }
    } catch (err) {
      // After a connection loss the item stays resent and the control is gone: nothing to show.
      if (!(err instanceof Error && err.message === CONNECTION_LOST)) {
        setMember(store, resentItemsAtom, key, false);
        setError(err instanceof Error ? err.message : String(err));
      }
    } finally {
      if (unconfirmed) setMember(store, resendingEntriesAtom, unconfirmed.client_id, false);
      inFlight.current = false;
      setBusy(false);
    }
  };
  return { busy, error, resend };
}

/**
 * Resend for an `undelivered` message of the current chat. The composer lists no unconfirmed
 * entry for a message that has this control (`composerUnconfirmedAtom`), so each lost message has
 * exactly one Resend.
 */
export function ResendControl({ item, turn }: { item: UserItem; turn: Turn }): JSX.Element | null {
  const may = useMayResend(item, turn);
  const online = useAtomValue(connectionAtom).phase === "open";
  const { busy, error, resend } = useResend(item, turn);
  // The control counts as resent once issued; it stays visible while that resend is in flight.
  if (!may && !busy) return null;
  return (
    <>
      <DisabledReason
        reason={busy ? "Resending" : online ? null : RESEND_OFFLINE}
        onClick={() => void resend()}
        size="sm"
      >
        Resend
      </DisabledReason>
      <span role="status">{error !== null && <Untrusted text={error} />}</span>
    </>
  );
}
