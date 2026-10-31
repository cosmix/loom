import { atom, useAtomValue, useStore } from "jotai";
import { useEffect, useRef, useState } from "react";

import type { DocView } from "@/plan/api/schema-files";
import {
  addAttachmentAtom,
  MAX_ATTACHMENT_BYTES,
  pendingAttachmentsAtom,
  type AttachmentDraft,
} from "@/plan/attachments";
import { fetchDoc } from "@/plan/files/api";
import type { ReviewMessage } from "@/plan/review/api";
import {
  authorBlockReasonAtom,
  authoringAtom,
  canAuthorAtom,
  isHolderAtom,
  planSocketAtom,
} from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { LoadState } from "@/plan/ui/load-state";
import type { Store } from "@/state/store";
import { chipLabel, summaryIdOf } from "./chips";
import { answerSuggestion } from "./commands";
import { ATTACHMENT_LIMIT_NOTE, draftAtom, MAX_ATTACHMENTS, mergeAttachments } from "./drafts";
import { Untrusted } from "./visible-controls";

type Suggestion = Extract<ReviewMessage, { kind: "suggestion" }>;

const suggestionsAtom = atom<Suggestion[]>([]);

/**
 * Keeps the pending suggestions in the store so frames arriving while the panel is collapsed or
 * the sheet is closed are not lost. Mount it once, beside the panel, in a component that stays
 * mounted. A resync or a replaced socket resets the list: status frames sent meanwhile are gone.
 * So does a change of the authoring holder: the server empties its box on every take and free and
 * tells only the senders, so a card kept across one would be refused as unknown when answered.
 */
export function useSuggestionFeed(): void {
  const store = useStore();
  useEffect(() => {
    const attach = () => {
      store.set(suggestionsAtom, []);
      const socket = store.get(planSocketAtom);
      if (!socket) return () => {};
      const off = socket.subscribe("review", (message) => {
        if (message.kind === "suggestion") {
          store.set(suggestionsAtom, (all) => [message, ...all.filter((i) => i.id !== message.id)]);
        } else if (message.kind === "suggestion_status" && message.state !== "sent") {
          store.set(suggestionsAtom, (all) => all.filter((item) => item.id !== message.id));
        }
      });
      const offResync = socket.onResync(() => store.set(suggestionsAtom, []));
      return () => {
        off();
        offResync();
      };
    };
    let detach = attach();
    const unwatch = store.sub(planSocketAtom, () => {
      detach();
      detach = attach();
    });
    const holder = () => store.get(authoringAtom)?.holder?.public_id ?? null;
    let holding = holder();
    const unwatchHolder = store.sub(authoringAtom, () => {
      if (holder() === holding) return;
      holding = holder();
      store.set(suggestionsAtom, []);
    });
    return () => {
      unwatch();
      unwatchHolder();
      detach();
    };
  }, [store]);
}

function currentAttachment(suggestion: Suggestion, doc: DocView): AttachmentDraft {
  const passage = suggestion.passage;
  if (doc.too_large || doc.truncated) throw new Error("The document is too large to attach safely");
  const index = passage.text ? doc.text.indexOf(passage.text) : -1;
  const section = doc.sections.find((item) => item.anchor === passage.anchor);
  if (index < 0 && !section) throw new Error("The suggested section no longer exists");
  const text =
    index >= 0
      ? passage.text
      : doc.text
          .split("\n")
          .slice(section!.start_line - 1, section!.end_line)
          .join("\n");
  if (new TextEncoder().encode(text).length > MAX_ATTACHMENT_BYTES) {
    throw new Error("The passage exceeds the attachment size limit");
  }
  // Suggestions carry no original hash. An empty hash deliberately retains the changed warning.
  const hash = index >= 0 ? doc.hash : "";
  if (passage.summary_sentence !== null) {
    return {
      kind: "summary",
      summary: summaryIdOf(passage.path),
      sentence: passage.summary_sentence,
      text,
      restsOn: [],
      hash,
    };
  }
  const start = index >= 0 ? doc.text.slice(0, index).split("\n").length : section!.start_line;
  const lines =
    passage.lines !== null && index >= 0
      ? { start, end: start + text.replace(/\n$/, "").split("\n").length - 1 }
      : null;
  return {
    kind: "source",
    path: passage.path,
    anchor: passage.anchor || null,
    text,
    lines,
    stageId: passage.stage,
    hash,
  };
}

function fillDraft(store: Store, suggestion: Suggestion, doc: DocView): void {
  const attachment = currentAttachment(suggestion, doc);
  const draft = store.get(draftAtom);
  const pending = store.get(pendingAttachmentsAtom);
  const all = mergeAttachments(draft.attachments, pending);
  if (all.length >= MAX_ATTACHMENTS) throw new Error(ATTACHMENT_LIMIT_NOTE);
  store.set(pendingAttachmentsAtom, all);
  store.set(addAttachmentAtom, attachment);
  const request = `${suggestion.from} asks: ${suggestion.note}`;
  store.set(draftAtom, (current) => ({
    text: current.text ? `${current.text}\n\n${request}` : request,
    attachments: store.get(pendingAttachmentsAtom),
  }));
}

function useSuggestionAnswer(suggestion: Suggestion) {
  const store = useStore();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const active = useRef(false);
  const busy = useRef(false);
  const filled = useRef(false);
  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);
  const answer = async (action: "attach" | "dismiss") => {
    if (busy.current || !store.get(isHolderAtom) || !store.get(canAuthorAtom)) return;
    busy.current = true;
    setPending(true);
    setError(null);
    try {
      if (action === "attach" && !filled.current) {
        const result = await fetchDoc(suggestion.passage.path);
        if (!active.current || !store.get(isHolderAtom) || !store.get(canAuthorAtom)) return;
        if (!result.ok) throw new Error(result.message);
        fillDraft(store, suggestion, result.value);
        filled.current = true;
      }
      await answerSuggestion(store, suggestion.id, action);
    } catch (failure) {
      if (active.current) setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      busy.current = false;
      if (active.current) setPending(false);
    }
  };
  return { pending, error, answer };
}

function SuggestionCard({ suggestion }: { suggestion: Suggestion }) {
  const { pending, error, answer } = useSuggestionAnswer(suggestion);
  const authorReason = useAtomValue(authorBlockReasonAtom);
  const reason = pending ? "Answering suggestion" : authorReason;
  const label = chipLabel({
    ...suggestion.passage,
    kind: suggestion.passage.summary_sentence === null ? "source" : "summary",
  });
  return (
    <article className="chat-suggestion">
      <p className="break-words font-medium">
        <Untrusted text={suggestion.from} /> suggests: <Untrusted text={label} />
      </p>
      <p className="whitespace-pre-wrap break-words text-sm leading-normal">
        <Untrusted text={suggestion.note} />
      </p>
      <div className="chat-suggestion-actions">
        <DisabledReason
          reason={reason}
          size="sm"
          onClick={() => {
            void answer("attach");
          }}
        >
          Attach
        </DisabledReason>
        <DisabledReason
          reason={reason}
          size="sm"
          onClick={() => {
            void answer("dismiss");
          }}
        >
          Dismiss
        </DisabledReason>
      </div>
      {pending && (
        <p role="status" className="text-xs text-muted-foreground">
          Answering suggestion…
        </p>
      )}
      {error && (
        <>
          <LoadState state="error" title="Could not answer suggestion" />
          <p role="status" className="break-words text-xs">
            <Untrusted text={error} />
          </p>
        </>
      )}
    </article>
  );
}

export function SuggestionCards() {
  const suggestions = useAtomValue(suggestionsAtom);
  const holder = useAtomValue(isHolderAtom);
  const canAuthor = useAtomValue(canAuthorAtom);
  if (!holder || !canAuthor || suggestions.length === 0) return null;
  return (
    <details open className="min-h-0 shrink-0 text-[13px]">
      <summary className="cursor-pointer font-medium">Suggestions ({suggestions.length})</summary>
      <div
        role="region"
        aria-label="Pending suggestions"
        className="chat-suggestions max-h-[33vh] overflow-y-auto overscroll-contain"
      >
        {suggestions.map((suggestion) => (
          <SuggestionCard key={suggestion.id} suggestion={suggestion} />
        ))}
      </div>
    </details>
  );
}
