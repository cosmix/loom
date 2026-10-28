import { getDefaultStore, useAtomValue, useStore } from "jotai";
import { useId } from "react";

import type { Attachment } from "@/plan/api/schema-chat";
import {
  authorBlockReasonAtom,
  authoringAtom,
  canAuthorAtom,
  connectionAtom,
  serverInfoAtom,
  stoppingAtom,
} from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { PendingChips } from "./chips";
import { composerBlock } from "./composer-gate";
import {
  useClearFocusRequest,
  useComposerBox,
  useDeliveredAttachments,
  useFirstHoldNote,
  useSendDraft,
} from "./composer-hooks";
import { draftAtom } from "./drafts";
import { formatMessage } from "./format-attachments";
import {
  composerUnconfirmedAtom,
  pendingPermissionsAtom,
  RESEND_OFFLINE,
  sessionAtom,
} from "./model";
import { CONTROLS } from "./model-identity";
import { PermissionCard } from "./permission-card";
import { QueueStrip } from "./queue-strip";
import { StopControl } from "./stop-control";
import { storageFullAtom } from "./storage";
import { SuggestionCards } from "./suggestion-cards";
import { Untrusted } from "./visible-controls";

const CAP = 100_000;
const HOLD_NOTE =
  "Closing this tab keeps the session for 10 minutes. After that it writes its notes into the documents and ends.";

function messageProblem(text: string, attachments: Attachment[]): string | null {
  const formatted = formatMessage(text, attachments);
  // The pane strips C0/C1 controls after formatting, preserving newline and tab.
  const delivered = formatted.replace(CONTROLS, "");
  const bytes = new TextEncoder().encode(delivered).byteLength;
  const clean = text.replace(CONTROLS, "").replace(/^[\p{White_Space}\uFEFF]+/u, "");
  if (clean.startsWith("/") && clean.split(/\p{White_Space}+/u)[0] !== "/compact") {
    return "Slash commands are not available here; `/compact` is.";
  }
  if (bytes > CAP)
    return `Message with attachments is ${bytes - CAP} byte${bytes - CAP === 1 ? "" : "s"} over the 100,000-byte limit.`;
  if (!delivered.trim()) return "Write a message or attach a passage";
  return null;
}

function ReadOnlyLine() {
  const server = useAtomValue(serverInfoAtom);
  const holder = useAtomValue(authoringAtom)?.holder;
  const draft = useAtomValue(draftAtom);
  const problem = server?.read_only[0];
  return (
    <div className="text-xs text-muted-foreground">
      <p>
        {problem ? (
          <>
            Read-only: <Untrusted text={problem.message} />
          </>
        ) : holder ? (
          <>
            Read-only · <Untrusted text={holder.name} /> is authoring
          </>
        ) : (
          "Read-only"
        )}
      </p>
      {(draft.text || draft.attachments.length > 0) && <p>Your draft is kept</p>}
    </div>
  );
}

function useBlockReasons(pending: boolean) {
  const draft = useAtomValue(draftAtom);
  const session = useAtomValue(sessionAtom);
  const stopping = useAtomValue(stoppingAtom);
  const server = useAtomValue(serverInfoAtom);
  const authorReason = useAtomValue(authorBlockReasonAtom);
  const connection = useAtomValue(connectionAtom);
  const attachments = useDeliveredAttachments();
  const blocked =
    composerBlock(session, { stopping, readOnly: !!server?.read_only.length }) ?? authorReason;
  const reason =
    blocked ??
    (connection.phase !== "open" ? "Not connected; the message is kept" : null) ??
    (pending ? "Sending" : messageProblem(draft.text, attachments));
  return { reason, resendBlock: reason === "Sending" ? reason : blocked };
}

function StorageNotes() {
  const localFull = useAtomValue(storageFullAtom);
  const globalFull = useAtomValue(storageFullAtom, { store: getDefaultStore() });
  const firstHold = useFirstHoldNote();
  return (
    <>
      {(localFull || globalFull) && (
        <p className="text-xs text-muted-foreground">
          Unsaved text is not being kept: browser storage is full
        </p>
      )}
      {firstHold && <p className="text-xs text-muted-foreground">{HOLD_NOTE}</p>}
    </>
  );
}

function ComposerInput() {
  const store = useStore();
  const draft = useAtomValue(draftAtom);
  const { pending, error, send } = useSendDraft();
  const { reason, resendBlock } = useBlockReasons(pending);
  const ref = useComposerBox(draft.text);
  const id = useId();
  return (
    <div className="shrink-0 space-y-2">
      <PendingChips />
      <textarea
        ref={ref}
        aria-label="Message"
        rows={Math.min(8, Math.max(1, draft.text.split("\n").length))}
        value={draft.text}
        aria-describedby={error ? id : undefined}
        className="block w-full resize-none overflow-y-auto rounded border border-border bg-background p-2 font-sans text-sm leading-[1.5] focus-visible:outline-2 focus-visible:outline-ring"
        style={{ maxHeight: "186px" }}
        onChange={(event) => store.set(draftAtom, (old) => ({ ...old, text: event.target.value }))}
        onKeyDown={(event) => {
          if (event.key !== "Enter" || event.shiftKey || event.nativeEvent.isComposing) return;
          event.preventDefault();
          if (!reason) void send();
        }}
      />
      <DisabledReason reason={reason} onClick={() => void send()}>
        Send
      </DisabledReason>
      {error && (
        <p id={id} role="status" className="text-xs text-muted-foreground">
          <Untrusted text={error} />
        </p>
      )}
      <StorageNotes />
      <UnconfirmedSends blocked={resendBlock} resend={(clientId) => void send(clientId)} />
    </div>
  );
}

function UnconfirmedSends({
  blocked,
  resend,
}: {
  blocked: string | null;
  resend: (id: string) => void;
}) {
  // A message the transcript lists as undelivered carries its own Resend; offering a second one
  // here would send it twice.
  const sends = useAtomValue(composerUnconfirmedAtom);
  const connection = useAtomValue(connectionAtom);
  return (
    <div className="max-h-24 overflow-y-auto text-xs">
      {sends.map((item) => (
        <div key={item.client_id}>
          <p className="text-(--tone-warning)">
            △ Not delivered · <Untrusted text={item.text} />
          </p>
          <DisabledReason
            reason={blocked ?? (connection.phase !== "open" ? RESEND_OFFLINE : null)}
            onClick={() => resend(item.client_id)}
            size="sm"
          >
            Resend
          </DisabledReason>
        </div>
      ))}
    </div>
  );
}

export function ComposerArea() {
  useClearFocusRequest();
  const canAuthor = useAtomValue(canAuthorAtom);
  const authoring = useAtomValue(authoringAtom);
  const server = useAtomValue(serverInfoAtom);
  const session = useAtomValue(sessionAtom);
  const permissions = useAtomValue(pendingPermissionsAtom);
  const viewer = !!server?.read_only.length || (!canAuthor && !!authoring?.holder);
  return (
    <div className="flex min-h-0 shrink-0 flex-col gap-2 border-t border-border p-3 font-sans">
      <div className="max-h-[33vh] min-h-0 overflow-y-auto space-y-2">
        <QueueStrip />
        {permissions[0] && (
          <div>
            <PermissionCard key={permissions[0].request_id} item={permissions[0]} inline={false} />
          </div>
        )}
        <SuggestionCards />
      </div>
      {viewer ? (
        <ReadOnlyLine />
      ) : session?.state === "ended" ? (
        <p className="text-xs text-muted-foreground">Chat ended. Start a new chat to continue.</p>
      ) : (
        <ComposerInput />
      )}
      {!server?.read_only.length && <StopControl />}
    </div>
  );
}
