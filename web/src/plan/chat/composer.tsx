import { useAtomValue, useStore } from "jotai";
import { ArrowUp, Paperclip } from "lucide-react";
import { useCallback, useEffect, useId, useRef, type ReactNode } from "react";

import {
  authorBlockReasonAtom,
  authoringAtom,
  canAuthorAtom,
  connectionAtom,
  serverInfoAtom,
  stoppingAtom,
} from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { AttachDocument } from "./attach-document";
import { PendingChips } from "./chips";
import { composerBlock } from "./composer-gate";
import { ConcludedChat, DraftKept, EndingChat, NewChatBesideBox } from "./composer-concluded";
import { ComposerFoot } from "./composer-foot";
import {
  composeAfterAtom,
  focusComposerAtom,
  useClearFocusRequest,
  useComposerBox,
  useDeliveredAttachments,
  useFileIntake,
  useSendDraft,
} from "./composer-hooks";
import { ContextLine } from "./context-line";
import { ContextPrompts } from "./context-prompts";
import { draftAtom } from "./drafts";
import { messageProblem } from "./message-problem";
import { composerUnconfirmedAtom, RESEND_OFFLINE, sessionAtom } from "./model";
import { ConfigBlockNotice } from "./pressure/config-block-notice";
import { codexRunsAlone } from "./pressure/steps";
import { pressureRunAtom } from "./pressure/use-pressure";
import { QueueStrip } from "./queue-strip";
import { shownSessionAtom } from "./shown-session";
import { isActiveSession, screenAtom } from "./session-home";
import { StopControl } from "./stop-control";
import { SuggestionCards } from "./suggestion-cards";
import { contextDocumentAtom } from "./viewed-document";
import { Untrusted, visibleControls } from "./visible-controls";
import "./chat-surface.css";

/** The banner carries the server's message and fix; this line names the state once. */
function ReadOnlyLine() {
  const holder = useAtomValue(authoringAtom)?.holder;
  const serverReadOnly = !!useAtomValue(serverInfoAtom)?.read_only.length;
  return (
    <div>
      <p className="chat-composer-note">
        {holder && !serverReadOnly ? (
          <>
            Read-only · <Untrusted text={holder.name} /> is authoring
          </>
        ) : (
          "Read-only"
        )}
      </p>
      <DraftKept />
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

/** The text box and its placeholder, which names the page the context chip shows. */
function MessageBox({
  reason,
  send,
  describedBy,
}: {
  reason: string | null;
  send: () => Promise<void>;
  describedBy: string | undefined;
}) {
  const store = useStore();
  const draft = useAtomValue(draftAtom);
  const ref = useComposerBox(draft.text);
  const context = useAtomValue(contextDocumentAtom);
  // Where the screen has no feature (the kickoff screen), a page-less placeholder would name
  // something that is not there.
  const inFeature = useAtomValue(screenAtom)?.feature != null;
  const placeholder =
    context.kind !== null
      ? `Ask about ${visibleControls(context.name)}…`
      : inFeature
        ? "Ask about this feature"
        : undefined;
  return (
    <textarea
      ref={ref}
      aria-label="Message"
      placeholder={placeholder}
      rows={Math.min(8, Math.max(1, draft.text.split("\n").length))}
      value={draft.text}
      aria-describedby={describedBy}
      className="chat-composer-field font-sans"
      style={{ maxHeight: "186px" }}
      onChange={(event) => store.set(draftAtom, (old) => ({ ...old, text: event.target.value }))}
      onKeyDown={(event) => {
        if (event.key !== "Enter" || event.shiftKey || event.nativeEvent.isComposing) return;
        event.preventDefault();
        if (!reason) void send();
      }}
    />
  );
}

/**
 * The bordered box the user types in: what the next message carries (the context chip, the
 * pending chips) above the text, and the drop target. Files dropped or pasted anywhere on it
 * attach. While a file is dragged over it, "Drop to attach" covers the box inside its own edge,
 * so nothing around it moves.
 */
function ComposerBox({ children }: { children: ReactNode }) {
  const { dragging, ...intake } = useFileIntake();
  return (
    <div className="chat-composer-box" data-dragging={dragging || undefined} {...intake}>
      <div className="chat-composer-attached">
        <ContextLine />
        <PendingChips />
      </div>
      {children}
      {dragging && (
        <p className="chat-drop-target">
          <Paperclip size={16} aria-hidden="true" />
          Drop to attach
        </p>
      )}
    </div>
  );
}

/**
 * The tools at the box's end, on the text's last line: the paperclip, then New chat (after a
 * concluded chat), Stop (while a turn runs; never for a pressure run, which stops from its own
 * head) and Send, round and last. Send stays enabled while a turn runs: a message sent then is
 * queued. Its reason, when blocked, is the foot line under the box (`footId`), not a line here.
 */
function ComposerActions({
  afterConcluded,
  reason,
  footId,
  send,
}: {
  afterConcluded: boolean;
  reason: string | null;
  footId: string;
  send: () => Promise<void>;
}) {
  const session = useAtomValue(sessionAtom);
  return (
    <div className="chat-composer-actions">
      <AttachDocument />
      <div className="chat-composer-end">
        {afterConcluded && <NewChatBesideBox />}
        {session?.kind !== "pressure" && <StopControl />}
        <div className="chat-composer-send">
          <DisabledReason
            reason={reason}
            reasonId={footId}
            look="primary"
            shape="round"
            onClick={() => void send()}
          >
            <ArrowUp size={15} strokeWidth={2.4} aria-hidden="true" />
            <span className="sr-only">Send</span>
          </DisabledReason>
        </div>
      </div>
    </div>
  );
}

/** `afterConcluded`: the chat shown is over, so sending starts a new one and offers no Resend. */
function ComposerInput({ afterConcluded }: { afterConcluded: boolean }) {
  const { pending, error, send } = useSendDraft();
  const { reason, resendBlock } = useBlockReasons(pending);
  const shown = useAtomValue(shownSessionAtom);
  const id = useId();
  const noteId = `${id}-new-chat`;
  const footId = `${id}-foot`;
  const describedBy = [afterConcluded ? noteId : null, error ? id : null].filter(Boolean).join(" ");
  return (
    <div className="chat-composer-input">
      <ComposerBox>
        <MessageBox reason={reason} send={send} describedBy={describedBy || undefined} />
        <ComposerActions
          afterConcluded={afterConcluded}
          reason={reason}
          footId={footId}
          send={send}
        />
      </ComposerBox>
      <ComposerFoot footId={footId} reason={reason}>
        {afterConcluded && (
          <p id={noteId} className="plan-fine">
            Sending starts a new chat
          </p>
        )}
        {error && (
          <p id={id} role="status" className="chat-composer-status">
            <Untrusted text={error} />
          </p>
        )}
      </ComposerFoot>
      {/* A concluded chat takes no messages, so nothing is left to resend into. */}
      {(shown === null || isActiveSession(shown)) && (
        <UnconfirmedSends blocked={resendBlock} resend={(clientId) => void send(clientId)} />
      )}
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
    <div className="chat-unconfirmed">
      {sends.map((item) => (
        <div key={item.client_id}>
          <p>
            <span aria-hidden="true" className="text-(--tone-warning)">
              △
            </span>{" "}
            Not delivered · <Untrusted text={item.text} />
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

type View = "read-only" | "ending" | "concluded" | "composer";

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
  const shown = useAtomValue(shownSessionAtom);
  const composeAfter = useAtomValue(composeAfterAtom);
  const viewer = !!server?.read_only.length || (!canAuthor && !!authoring?.holder);
  const concluded = shown !== null && (shown.state === "ended" || shown.state === "failed");
  // A concluded chat shows New chat. A clicked prompt chooses to write after it instead, and the
  // choice belongs to that chat: the draft's words do not decide the view, so emptying the box
  // keeps it and a stale or returned draft never brings it back. Sending from it starts a new chat.
  const view: View = viewer
    ? "read-only"
    : shown?.state === "ending"
      ? "ending"
      : concluded && composeAfter !== shown.id
        ? "concluded"
        : "composer";
  const onLeave = useFocusHandoff(view);
  return (
    <div className="chat-composer font-sans">
      <ConfigBlockNotice context="chat" />
      {view !== "read-only" && view !== "ending" && <ContextPrompts />}
      <div className="chat-composer-extras">
        <QueueStrip />
        <SuggestionCards />
      </div>
      {view === "read-only" ? (
        <ReadOnlyLine />
      ) : view === "ending" ? (
        <EndingChat />
      ) : view === "concluded" ? (
        <>
          {/* An attachment made on a concluded session's own screen waits for the next chat. */}
          {canAuthor && <PendingChips />}
          <ConcludedChat onLeave={onLeave} />
          <DraftKept />
        </>
      ) : (
        <ComposerInput afterConcluded={concluded} />
      )}
    </div>
  );
}
