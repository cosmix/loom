import { getDefaultStore, useAtomValue } from "jotai";
import type { ReactNode } from "react";

import type { SessionView } from "@/plan/api/schema-chat";
import { useFirstHoldNote } from "./composer-hooks";
import { sessionAtom } from "./model";
import { storageFullAtom } from "./storage";
import { Untrusted } from "./visible-controls";

const HOLD_NOTE =
  "Closing this tab keeps the session for 10 minutes. After that it writes its notes into the documents and ends.";

function StorageNotes() {
  const localFull = useAtomValue(storageFullAtom);
  const globalFull = useAtomValue(storageFullAtom, { store: getDefaultStore() });
  const firstHold = useFirstHoldNote();
  return (
    <>
      {(localFull || globalFull) && (
        <p className="chat-composer-note">
          Unsaved text is not being kept: browser storage is full
        </p>
      )}
      {firstHold && <p className="chat-composer-note">{HOLD_NOTE}</p>}
    </>
  );
}

/** The foot's words while Send is open: a running turn or a pending decision queues the message. */
function hintFor(session: SessionView | null): string {
  return session?.state === "working" || session?.state === "permission"
    ? "Your message waits for the turn to finish"
    : "Enter to send";
}

/**
 * The fine line under the composer box: Send's reason while it is blocked (the line is Send's
 * accessible description through `footId`, so the reason is drawn once), else the hint. The
 * composer's notes (`children`, then the storage and first-hold notes) follow it.
 */
export function ComposerFoot({
  footId,
  reason,
  children,
}: {
  footId: string;
  reason: string | null;
  children: ReactNode;
}) {
  const session = useAtomValue(sessionAtom);
  return (
    <div className="chat-composer-foot">
      <p id={footId} className="chat-composer-hint">
        {reason === null ? hintFor(session) : <Untrusted text={reason} />}
      </p>
      {children}
      <StorageNotes />
    </div>
  );
}
