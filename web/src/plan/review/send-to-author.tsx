import { Provider, useAtomValue, useStore } from "jotai";
import { useEffect, useId, useRef, useState } from "react";
import { createRoot, type Root } from "react-dom/client";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import type { HolderView } from "@/plan/api/schema";
import { registerSelectionSecondary, type AttachmentDraft } from "@/plan/attachments";
import { suggest, toAttachmentInput } from "@/plan/chat/commands";
import { Untrusted } from "@/plan/chat/visible-controls";
import { authoringAtom, canAuthorAtom, planSocketAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";

import type { ReviewMessage, SuggestedPassage } from "./api";

type Target = { public_id: string; name: string };
type Status = Extract<ReviewMessage, { kind: "suggestion_status" }>;

function passageOf(draft: AttachmentDraft): SuggestedPassage {
  const input = toAttachmentInput(draft);
  return {
    path: input.path,
    anchor: input.anchor ?? "",
    text: draft.text,
    lines: input.lines,
    stage: input.stage,
    summary_sentence: input.summary_sentence,
  };
}

function statusText(status: Status): string {
  if (status.state === "sent") return `Sent to ${status.holder}`;
  if (status.state === "attached") return "Attached";
  if (status.state === "dismissed") return "Dismissed";
  return "Dropped: authoring changed hands";
}

function useSender(passage: SuggestedPassage) {
  const store = useStore();
  const socket = useAtomValue(planSocketAtom);
  const tracking = useRef<{ pending: boolean; id: string | null }>({ pending: false, id: null });
  const [status, setStatus] = useState<Status | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!socket) return;
    return socket.subscribe("review", (reply) => {
      if (reply.kind !== "suggestion_status") return;
      const current = tracking.current;
      if (reply.state === "sent" && current.pending && current.id === null) current.id = reply.id;
      if (reply.id === current.id) setStatus(reply);
    });
  }, [socket]);
  async function submit(note: string) {
    if (tracking.current.pending || tracking.current.id !== null) return;
    tracking.current.pending = true;
    setPending(true);
    setError(null);
    try {
      await suggest(store, passage, note);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      tracking.current.pending = false;
      setPending(false);
    }
  }
  return { status, pending, error, submit };
}

function NoteForm({
  disabled,
  pending,
  onSend,
}: {
  disabled: boolean;
  pending: boolean;
  onSend: (note: string) => Promise<void>;
}) {
  const [note, setNote] = useState("");
  const id = useId();
  const length = [...note].length;
  const tooLong = length > 1_000;
  return (
    <form
      className="grid gap-2 text-[13px]"
      onSubmit={(event) => {
        event.preventDefault();
        if (!disabled && !pending && !tooLong) void onSend(note);
      }}
    >
      <label htmlFor={id}>Note (optional)</label>
      <textarea
        id={id}
        value={note}
        onChange={(event) => setNote(event.target.value)}
        aria-invalid={tooLong}
        aria-describedby={`${id}-limit`}
        disabled={disabled || pending}
        className="min-h-24 rounded border bg-background p-2 text-sm leading-normal"
      />
      <p id={`${id}-limit`} className={tooLong ? "text-destructive" : "text-muted-foreground"}>
        {length.toLocaleString("en-US")} / 1,000 characters
        {tooLong ? "; shorten your note to send it." : ""}
      </p>
      <Button type="submit" size="sm" disabled={disabled || pending || tooLong}>
        {pending ? "Sending…" : "Send"}
      </Button>
    </form>
  );
}

function SendStatus({
  status,
  away,
  changed,
  error,
}: {
  status: Status | null;
  away: string | null;
  changed: boolean;
  error: string | null;
}) {
  return (
    <>
      {status && (
        <p role="status" className="text-[13px]">
          <Untrusted text={statusText(status)} />
        </p>
      )}
      {away && (
        <p role="status" className="text-[13px]">
          <Untrusted text={`${away} is away`} />
        </p>
      )}
      {!status && changed && (
        <p role="status">Authoring changed hands; select the passage again.</p>
      )}
      {error && (
        <p role="alert">
          <Untrusted text={error} />
        </p>
      )}
    </>
  );
}

function awayHolder(status: Status | null, holder: HolderView | null | undefined): string | null {
  if (status?.state === "sent") return status.away ? status.holder : null;
  return holder?.away_since ? holder.name : null;
}

function SendDialog({
  draft,
  target,
  onClose,
}: {
  draft: AttachmentDraft;
  target: Target;
  onClose: () => void;
}) {
  const holder = useAtomValue(authoringAtom)?.holder;
  const canAuthor = useAtomValue(canAuthorAtom);
  const [passage] = useState(() => passageOf(draft));
  const sender = useSender(passage);
  const changed = holder?.public_id !== target.public_id || canAuthor;
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent className="motion-reduce:animate-none">
        <DialogTitle>Send to author</DialogTitle>
        <DialogDescription>
          Passage:{" "}
          <Untrusted text={`${passage.path}${passage.anchor ? `#${passage.anchor}` : ""}`} />
        </DialogDescription>
        <blockquote className="max-h-32 overflow-auto text-sm leading-normal">
          <Untrusted text={passage.text} />
        </blockquote>
        <NoteForm
          disabled={changed || sender.status !== null}
          pending={sender.pending}
          onSend={sender.submit}
        />
        <SendStatus
          status={sender.status}
          away={awayHolder(sender.status, holder)}
          changed={changed}
          error={sender.error}
        />
      </DialogContent>
    </Dialog>
  );
}

function SendButton({
  draft,
  open,
}: {
  draft: AttachmentDraft;
  open: (store: Store, draft: AttachmentDraft, holder: Target) => void;
}) {
  const store = useStore();
  const canAuthor = useAtomValue(canAuthorAtom);
  const holder = useAtomValue(authoringAtom)?.holder;
  if (canAuthor || !holder) return null;
  return (
    <Button type="button" size="sm" onClick={() => open(store, draft, holder)}>
      Send to author
    </Button>
  );
}

export function registerSendToAuthor(): () => void {
  let root: Root | null = null;
  let host: HTMLDivElement | null = null;
  let sequence = 0;
  // The selection slot disappears when focus moves into the form. Its dialog must outlive it.
  const open = (store: Store, draft: AttachmentDraft, target: Target) => {
    if (!root) {
      host = document.createElement("div");
      document.body.append(host);
      root = createRoot(host);
    }
    const dialogRoot = root;
    dialogRoot.render(
      <Provider store={store}>
        <SendDialog
          key={++sequence}
          draft={draft}
          target={target}
          onClose={() => dialogRoot.render(null)}
        />
      </Provider>,
    );
  };
  const unregister = registerSelectionSecondary((draft) => (
    <SendButton draft={draft} open={open} />
  ));
  return () => {
    unregister();
    root?.unmount();
    host?.remove();
    root = null;
    host = null;
  };
}
