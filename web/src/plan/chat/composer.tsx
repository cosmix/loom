import { getDefaultStore, useAtomValue, useStore } from "jotai";
import { useCallback, useEffect, useId, useLayoutEffect, useRef } from "react";

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
  focusComposerAtom,
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
import { NewChatOffers } from "./new-chat-offers";
import { PermissionCard } from "./permission-card";
import { codexRunsAlone } from "./pressure/steps";
import { pressureRunAtom } from "./pressure/use-pressure";
import { QueueStrip } from "./queue-strip";
import { shownSessionAtom } from "./shown-session";
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
  const codexAlone = codexRunsAlone(useAtomValue(pressureRunAtom));
  const blocked =
    composerBlock(session, { stopping, readOnly: !!server?.read_only.length, codexAlone }) ??
    authorReason;
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

/**
 * Section 08, Session states: Ended offers "New chat" and the primary offer, Failed "New chat".
 * Starting the next chat swaps this view for the composer, and the button that held focus leaves
 * with it; `onLeave` reports whether focus was inside so the text box can take it.
 */
function ConcludedChat({
  ended,
  onLeave,
}: {
  ended: boolean;
  onLeave: (hadFocus: boolean) => void;
}) {
  const area = useRef<HTMLDivElement>(null);
  // A layout cleanup runs before React detaches the button, so focus can still be read here.
  useLayoutEffect(() => {
    const node = area.current;
    return () => onLeave(!!node?.contains(document.activeElement));
  }, [onLeave]);
  return (
    <div ref={area} className="flex flex-wrap items-center gap-2">
      <NewChatOffers offer={ended} />
    </div>
  );
}

type View = "read-only" | "concluded" | "composer";

/**
 * Moves focus into the text box that replaces the concluded view when focus was on that view's
 * button. The request is the one the `c` shortcut makes; the composer takes it as it mounts.
 * Returns the `onLeave` callback for ConcludedChat.
 */
function useFocusHandoff(view: View): (hadFocus: boolean) => void {
  const store = useStore();
  const owed = useRef(false);
  const onLeave = useCallback((hadFocus: boolean) => {
    owed.current = hadFocus;
  }, []);
  useEffect(() => {
    const due = owed.current;
    owed.current = false;
    if (due && view === "composer") store.set(focusComposerAtom, true);
  }, [view, store]);
  return onLeave;
}

export function ComposerArea() {
  useClearFocusRequest();
  const canAuthor = useAtomValue(canAuthorAtom);
  const authoring = useAtomValue(authoringAtom);
  const server = useAtomValue(serverInfoAtom);
  const state = useAtomValue(shownSessionAtom)?.state;
  const session = useAtomValue(sessionAtom);
  const permissions = useAtomValue(pendingPermissionsAtom);
  const viewer = !!server?.read_only.length || (!canAuthor && !!authoring?.holder);
  const concluded = state === "ended" || state === "failed";
  const view: View = viewer ? "read-only" : concluded ? "concluded" : "composer";
  const onLeave = useFocusHandoff(view);
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
      {view === "read-only" ? (
        <ReadOnlyLine />
      ) : view === "concluded" ? (
        <ConcludedChat ended={state === "ended"} onLeave={onLeave} />
      ) : (
        <ComposerInput />
      )}
      {!server?.read_only.length && session?.kind !== "pressure" && <StopControl />}
    </div>
  );
}
