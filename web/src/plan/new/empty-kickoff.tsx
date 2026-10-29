import { useAtomValue, useStore } from "jotai";
import { useEffect, useId, useRef, useState } from "react";

import { startChat } from "@/plan/chat/commands";
import { sessionAtom } from "@/plan/chat/model";
import {
  confirmEndFirst,
  endCurrentChat,
  isActiveSession,
  startError,
} from "@/plan/chat/offer-flow";
import { Untrusted } from "@/plan/chat/visible-controls";
import { kickoffSessionAtom } from "@/plan/new/kickoff-state";
import { authorBlockReasonAtom, canAuthorAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { LoomDoodle } from "@/plan/ui/doodle";
import type { Store } from "@/state/store";
import { DocumentStarter, type StarterDocument } from "./document-starter";

import "./kickoff.css";

type PickedDocument = StarterDocument & { id: string };
type KickoffDraft = ReturnType<typeof useKickoffDraft>;

const STARTERS = [
  { label: "A new feature", text: "I want to build a new feature: ", small: false },
  { label: "A small fix or improvement", text: "I want to fix or improve: ", small: true },
  {
    label: "Grow my standalone plans into a feature",
    text: "I have standalone plans that belong together and want to grow them into a feature.",
    small: false,
  },
];

function waitForKickoffSession(
  store: Store,
  previousId: string | null,
  signal: AbortSignal,
  onTimeout: () => void,
) {
  return new Promise<void>((resolve) => {
    const cleanups: Array<() => void> = [];
    let finished = false;
    const finish = () => {
      if (finished) return;
      finished = true;
      cleanups.splice(0).forEach((stop) => stop());
      resolve();
    };
    const check = () => {
      if (signal.aborted) return finish();
      const session = store.get(sessionAtom);
      // An ended session may remain in the snapshot until its replacement arrives.
      if (session !== null && session.id !== previousId) {
        store.set(kickoffSessionAtom, session.id);
        finish();
      }
    };
    cleanups.push(store.sub(sessionAtom, check));
    // A slow snapshot can still arrive after the UI offers a retry.
    const timeout = setTimeout(onTimeout, 5000);
    cleanups.push(() => clearTimeout(timeout));
    const abort = () => finish();
    signal.addEventListener("abort", abort, { once: true });
    cleanups.push(() => signal.removeEventListener("abort", abort));
    check();
  });
}

async function beginKickoff(
  store: Store,
  small: boolean,
  text: string,
  documents: PickedDocument[],
  signal: AbortSignal,
  onTimeout: () => void,
) {
  const current = store.get(sessionAtom);
  if (isActiveSession(current))
    await endCurrentChat(store, current, signal, "start the conversation");
  if (signal.aborted) return;
  if (!store.get(canAuthorAtom)) {
    throw new Error(store.get(authorBlockReasonAtom) ?? "Authoring is unavailable");
  }
  store.set(kickoffSessionAtom, null);
  await startChat(
    store,
    small ? "kickoff-small" : "kickoff",
    text,
    documents.map((document) => document.attachment),
  );
  if (!signal.aborted) await waitForKickoffSession(store, current?.id ?? null, signal, onTimeout);
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
      if (current.id === store.get(kickoffSessionAtom) || !confirmEndFirst()) return;
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
  return { small, text, setText, documents, composer, choose, attach, remove };
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
    <div className="w-full">
      <label htmlFor={id} className="sr-only">
        What do you want to build?
      </label>
      <textarea
        id={id}
        ref={draft.composer}
        className="kickoff-composer"
        rows={5}
        value={draft.text}
        disabled={disabled}
        onChange={(event) => draft.setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key !== "Enter" || event.shiftKey || event.nativeEvent.isComposing) return;
          event.preventDefault();
          start();
        }}
      />
    </div>
  );
}

function KickoffAttachments({ draft, disabled }: { draft: KickoffDraft; disabled: boolean }) {
  if (!draft.documents.length) return null;
  return (
    <ul aria-label="Attached documents" className="flex w-full flex-wrap gap-2 text-sm">
      {draft.documents.map((document) => (
        <li
          key={document.id}
          className="flex min-w-0 items-center gap-3 rounded-md border border-border px-3 py-2"
        >
          <span className="min-w-0 break-words">
            Attached: <Untrusted text={document.label} />
          </span>
          <button
            type="button"
            disabled={disabled}
            className="shrink-0 underline focus-visible:outline-2 focus-visible:outline-ring"
            onClick={() => draft.remove(document.id)}
          >
            Remove
          </button>
        </li>
      ))}
    </ul>
  );
}

function KickoffStarters({ draft, disabled }: { draft: KickoffDraft; disabled: boolean }) {
  return (
    <fieldset disabled={disabled} className="kickoff-starters w-full space-y-2">
      <legend className="sr-only">Conversation starters</legend>
      {STARTERS.map((starter) => (
        <button
          key={starter.label}
          type="button"
          className="kickoff-starter"
          onClick={() => draft.choose(starter)}
        >
          {starter.label}
        </button>
      ))}
      <DocumentStarter onPick={draft.attach} />
    </fieldset>
  );
}

export function EmptyKickoff({ small = false }: { small?: boolean }) {
  const draft = useKickoffDraft(small);
  const { start, pending, error } = useKickoffStart();
  const canAuthor = useAtomValue(canAuthorAtom);
  const blockReason = useAtomValue(authorBlockReasonAtom);
  const disabled = !canAuthor || pending;
  const reason = !canAuthor
    ? "Authoring is unavailable"
    : pending
      ? "Starting the conversation"
      : !draft.text.trim() && !draft.documents.length
        ? "Describe what you want to build or attach a document"
        : null;
  const submit = () => {
    void start(draft.small, draft.text, draft.documents);
  };
  return (
    <section className="kickoff-empty" aria-label="Start something new">
      <LoomDoodle className="text-muted-foreground" />
      <h1 className="text-center text-3xl font-semibold tracking-tight">
        What do you want to build?
      </h1>
      <KickoffComposer draft={draft} disabled={disabled} start={submit} />
      <KickoffAttachments draft={draft} disabled={disabled} />
      <div className="w-full">
        <DisabledReason reason={reason} onClick={submit}>
          {error ? "Try again" : "Start the conversation"}
        </DisabledReason>
        {!canAuthor && blockReason && (
          <p className="mt-1 text-xs text-muted-foreground">
            <Untrusted text={blockReason} />
          </p>
        )}
      </div>
      {error && (
        <p role="status">
          <Untrusted text={error} />
        </p>
      )}
      <KickoffStarters draft={draft} disabled={disabled} />
      <p className="text-center text-sm text-muted-foreground">
        Nothing is written until you say so.
      </p>
    </section>
  );
}
