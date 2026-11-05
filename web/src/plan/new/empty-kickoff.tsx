import { useAtomValue, useStore } from "jotai";
import {
  File,
  FileImage,
  FileText,
  FileUp,
  FolderInput,
  Lightbulb,
  Paperclip,
  SendHorizontal,
  Wrench,
  X,
} from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";

import { previewUrl } from "@/plan/attachments/intake";
import { startChat } from "@/plan/chat/commands";
import { sessionAtom } from "@/plan/chat/model";
import { noPanelRefusal, startError } from "@/plan/chat/offer-flow";
import { isActiveSession } from "@/plan/chat/session-home";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { docRel } from "@/plan/files/address";
import { OpenDraftNotice } from "@/plan/new/draft-view";
import { KindControl } from "@/plan/new/kind-control";
import { kickoffSessionAtom, waitForKickoffSession } from "@/plan/new/kickoff-state";
import { authorBlockReasonAtom, canAuthorAtom, kickoffDraftsAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { LoomDoodle } from "@/plan/ui/doodle";
import { formatBytes } from "@/plan/ui/format-bytes";
import type { Store } from "@/state/store";
import { DocumentStarter, type StarterDocument } from "./document-starter";

import "@/plan/chat/chat-chips.css";
import "./kickoff.css";

type PickedDocument = StarterDocument & { id: string };
type KickoffDraft = ReturnType<typeof useKickoffDraft>;

const STARTERS = [
  {
    label: "A new feature for this project",
    text: "I want to build a new feature: ",
    small: false,
    Icon: Lightbulb,
  },
  {
    label: "A small fix or improvement",
    text: "I want to fix or improve: ",
    small: true,
    Icon: Wrench,
  },
  {
    label: "Grow standalone plans into a feature",
    text: "I have standalone plans that belong together and want to grow them into a feature.",
    small: false,
    Icon: FolderInput,
  },
];

async function beginKickoff(
  store: Store,
  small: boolean,
  text: string,
  documents: PickedDocument[],
  signal: AbortSignal,
  onTimeout: () => void,
) {
  // An ended session may remain in the snapshot; only a different one is this kickoff's.
  const previous = store.get(sessionAtom);
  store.set(kickoffSessionAtom, null);
  await startChat(
    store,
    small ? "kickoff-small" : "kickoff",
    text,
    documents.map((document) => document.attachment),
  );
  if (!signal.aborted) await waitForKickoffSession(store, previous?.id ?? null, signal, onTimeout);
}

function useKickoffStart() {
  const store = useStore();
  const request = useRef<AbortController | null>(null);
  const sessionWait = useRef<AbortController | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => () => sessionWait.current?.abort(), []);
  const start = async (small: boolean, text: string, documents: PickedDocument[]) => {
    if (request.current || !store.get(canAuthorAtom) || (!text.trim() && !documents.length)) return;
    const current = store.get(sessionAtom);
    if (isActiveSession(current)) {
      // This form shows only while the route shows no session, so a chat open now is not on this
      // screen and is not this screen's to end. The exception is this tab's own kickoff, whose
      // snapshot has arrived and is about to replace the form.
      if (current.id !== store.get(kickoffSessionAtom)) setError(noPanelRefusal(store));
      return;
    }
    sessionWait.current?.abort();
    const controller = new AbortController();
    request.current = controller;
    sessionWait.current = controller;
    setPending(true);
    setError(null);
    try {
      await beginKickoff(store, small, text, documents, controller.signal, () => {
        if (controller.signal.aborted) return;
        request.current = null;
        setPending(false);
        setError("The conversation did not start. Please try again.");
      });
      if (!controller.signal.aborted) setError(null);
    } catch (cause) {
      if (!controller.signal.aborted) setError(startError(store, cause));
    } finally {
      if (request.current === controller) request.current = null;
      if (!controller.signal.aborted) setPending(false);
    }
  };
  return { start, pending, error };
}

function useKickoffDraft(initialSmall: boolean) {
  const [small, setSmall] = useState(initialSmall);
  const [text, setText] = useState("");
  const [documents, setDocuments] = useState<PickedDocument[]>([]);
  const composer = useRef<HTMLTextAreaElement>(null);
  const choose = (starter: (typeof STARTERS)[number]) => {
    setSmall(starter.small);
    setText(starter.text);
    composer.current?.focus();
  };
  const attach = (document: StarterDocument) => {
    const picked = { ...document, id: crypto.randomUUID() };
    setDocuments((previous) => [...previous, picked]);
    composer.current?.focus();
  };
  const remove = (id: string) => setDocuments((previous) => previous.filter((d) => d.id !== id));
  return { small, setSmall, text, setText, documents, composer, choose, attach, remove };
}

function KickoffComposer({
  draft,
  disabled,
  start,
}: {
  draft: KickoffDraft;
  disabled: boolean;
  start: () => void;
}) {
  const id = useId();
  return (
    <>
      <label htmlFor={id} className="sr-only">
        What do you want to build?
      </label>
      <textarea
        id={id}
        ref={draft.composer}
        className="kickoff-composer"
        rows={3}
        value={draft.text}
        disabled={disabled}
        onChange={(event) => draft.setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key !== "Enter" || event.shiftKey || event.nativeEvent.isComposing) return;
          event.preventDefault();
          start();
        }}
      />
    </>
  );
}

/** The row's glyph: an image's own thumbnail, else a file's or a document's mark. */
function AttachmentGlyph({ document }: { document: PickedDocument }) {
  const image = document.file?.media_type.startsWith("image/") ?? false;
  const thumbnail = image && document.file ? previewUrl(document.file.id) : null;
  if (thumbnail !== null) return <img src={thumbnail} alt="" className="chat-chip-thumb" />;
  const Glyph = image ? FileImage : document.file ? File : FileText;
  return (
    <span aria-hidden="true" className="chat-chip-glyph">
      <Glyph />
    </span>
  );
}

/** What stands after the name: an upload's size, or a document's file name. */
function attachmentDetail(document: PickedDocument): string | null {
  if (document.file) return formatBytes(document.file.bytes);
  if (document.attachment.kind === "file") return null;
  return docRel(document.attachment.path).split("/").pop() ?? null;
}

/**
 * One attached document, drawn as the chat composer draws its rows (chat-chips.css): the glyph,
 * the name with its detail after it, and the remove at the row's end. "Attached:" is read, not
 * shown: on screen the row's place under the field says it.
 */
function KickoffAttachment({
  document,
  disabled,
  onRemove,
}: {
  document: PickedDocument;
  disabled: boolean;
  onRemove: () => void;
}) {
  const nameId = useId();
  const label = visibleControls(document.label);
  const detail = attachmentDetail(document);
  return (
    <li className="chat-chip" data-kind={document.file ? "file" : undefined}>
      <AttachmentGlyph document={document} />
      <span id={nameId} className="chat-chip-label" title={label}>
        <span className="sr-only">Attached: </span>
        <Untrusted text={document.label} />
      </span>
      {detail !== null && (
        <span className={document.file ? "chat-chip-size" : "chat-chip-source"}>
          <Untrusted text={detail} />
        </span>
      )}
      {/* The row's name is the button's description; its own name is the bare verb, which the
          screen's tests query. */}
      <button
        type="button"
        className="chat-chip-remove"
        aria-label="Remove"
        aria-describedby={nameId}
        disabled={disabled}
        onClick={onRemove}
      >
        <X size={14} aria-hidden="true" />
      </button>
    </li>
  );
}

function KickoffAttachments({ draft, disabled }: { draft: KickoffDraft; disabled: boolean }) {
  if (!draft.documents.length) return null;
  return (
    <ul aria-label="Attached documents" className="kickoff-attachments chat-chip-list">
      {draft.documents.map((document) => (
        <KickoffAttachment
          key={document.id}
          document={document}
          disabled={disabled}
          onRemove={() => draft.remove(document.id)}
        />
      ))}
    </ul>
  );
}

function KickoffStarters({ draft, disabled }: { draft: KickoffDraft; disabled: boolean }) {
  return (
    <fieldset disabled={disabled} className="kickoff-starters">
      <legend className="sr-only">Conversation starters</legend>
      {STARTERS.map((starter) => (
        <button
          key={starter.label}
          type="button"
          data-look="secondary"
          className="plan-button kickoff-starter"
          onClick={() => draft.choose(starter)}
        >
          <starter.Icon aria-hidden="true" />
          {starter.label}
        </button>
      ))}
      <DocumentStarter onPick={draft.attach} look="secondary" className="kickoff-starter">
        <FileUp aria-hidden="true" />
        Start from a document I have
      </DocumentStarter>
    </fieldset>
  );
}

function KickoffCard({
  draft,
  disabled,
  reason,
  error,
  submit,
}: {
  draft: KickoffDraft;
  disabled: boolean;
  reason: string | null;
  error: string | null;
  submit: () => void;
}) {
  return (
    <div className="kickoff-card">
      <KickoffComposer draft={draft} disabled={disabled} start={submit} />
      <KickoffAttachments draft={draft} disabled={disabled} />
      <div className="kickoff-actions">
        <DocumentStarter onPick={draft.attach} variant="ghost" look="quiet" disabled={disabled}>
          <Paperclip aria-hidden="true" />
          Attach notes or a document
        </DocumentStarter>
        <div className="kickoff-start">
          <DisabledReason reason={reason} look="primary" onClick={submit}>
            <SendHorizontal aria-hidden="true" />
            {error ? (
              "Try again"
            ) : (
              <>
                Start <span className="sr-only">the conversation</span>
              </>
            )}
          </DisabledReason>
        </div>
      </div>
    </div>
  );
}

function KickoffTitle() {
  return (
    <header className="kickoff-title">
      <p className="plan-eyebrow">New work</p>
      <h1 className="plan-page-title">What do you want to build?</h1>
      <p className="kickoff-lead">
        Describe it the way you would to a colleague: the problem, who has it, what you have in
        mind. Loom asks questions, pushes back, and suggests a name and the sections a spec needs.
      </p>
    </header>
  );
}

export function EmptyKickoff({ small = false }: { small?: boolean }) {
  const draft = useKickoffDraft(small);
  const { start, pending, error } = useKickoffStart();
  const canAuthor = useAtomValue(canAuthorAtom);
  const blockReason = useAtomValue(authorBlockReasonAtom);
  // The form is always empty; a kickoff still running is offered above it, never reopened here.
  const open = useAtomValue(kickoffDraftsAtom).find((entry) => entry.live);
  const disabled = !canAuthor || pending;
  const reason = !canAuthor
    ? (blockReason ?? "Authoring is unavailable")
    : pending
      ? "Starting the conversation"
      : !draft.text.trim() && !draft.documents.length
        ? "Describe what you want to build or attach a document"
        : null;
  const submit = () => {
    void start(draft.small, draft.text, draft.documents);
  };
  // The same page grid as the live conversation (kickoff-route.tsx): the head sits where the
  // conversation's head will, and the doodle holds the column Taking shape takes over.
  return (
    <div className="kickoff-page" data-state="empty">
      <section className="kickoff-main kickoff-empty" aria-label="Start something new">
        <KickoffTitle />
        {open && <OpenDraftNotice draft={open} />}
        <KindControl small={draft.small} onChange={draft.setSmall} disabled={disabled} />
        <KickoffCard
          draft={draft}
          disabled={disabled}
          reason={reason}
          error={error}
          submit={submit}
        />
        {error && (
          <p role="status" className="kickoff-error">
            <Untrusted text={error} />
          </p>
        )}
        <KickoffStarters draft={draft} disabled={disabled} />
        <p className="plan-fine kickoff-note">Nothing is written until you say so.</p>
      </section>
      <div className="kickoff-side kickoff-side--art">
        <LoomDoodle className="text-muted-foreground" />
      </div>
    </div>
  );
}
