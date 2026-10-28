import { atom, useAtomValue, useStore } from "jotai";
import { useEffect, useRef, useState } from "react";

import type { DocView } from "@/plan/api/schema-files";
import {
  addAttachmentAtom,
  pendingAttachmentsAtom,
  type AttachmentDraft,
} from "@/plan/attachments";
import { MAX_ATTACHMENT_BYTES } from "@/plan/attachments/add-to-chat";
import { docRel } from "@/plan/files/address";
import { fetchDoc } from "@/plan/files/api";
import type { ReviewMessage } from "@/plan/review/api";
import { hashesAtom } from "@/plan/review/live";
import {
  authorBlockReasonAtom,
  canAuthorAtom,
  isHolderAtom,
  planSocketAtom,
} from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { LoadState } from "@/plan/ui/load-state";
import type { Store } from "@/state/store";
import { chipLabel } from "./chips";
import { answerSuggestion } from "./commands";
import { draftAtom } from "./drafts";
import { Untrusted } from "./visible-controls";

type Suggestion = Extract<ReviewMessage, { kind: "suggestion" }>;

const suggestionsAtom = atom<Suggestion[]>([]);

/**
 * Keeps the pending suggestions in the store so frames arriving while the panel is collapsed or
 * the sheet is closed are not lost. Mount it once, beside the panel, in a component that stays
 * mounted. A resync or a replaced socket resets the list: status frames sent meanwhile are gone.
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
    return () => {
      unwatch();
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
      summary: docRel(passage.path)
        .replace(/^loom\/summaries\//, "")
        .replace(/\.md$/, ""),
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
  const all = [
    ...draft.attachments,
    ...pending.filter(
      (item) => !draft.attachments.some((saved) => JSON.stringify(saved) === JSON.stringify(item)),
    ),
  ];
  if (all.length >= 10) throw new Error("At most 10 attachments per message");
  store.set(pendingAttachmentsAtom, all);
  store.set(addAttachmentAtom, attachment);
  store.set(hashesAtom, (current) => ({
    head: current?.head ?? null,
    docs: { ...current?.docs, [suggestion.passage.path]: doc.hash },
  }));
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
    <article className="min-w-0 space-y-2 rounded border border-border p-2 text-[13px]">
      <p className="break-words font-medium">
        <Untrusted text={suggestion.from} /> suggests: <Untrusted text={label} />
      </p>
      <p className="whitespace-pre-wrap break-words text-sm leading-normal">
        <Untrusted text={suggestion.note} />
      </p>
      <div className="flex gap-2">
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
      {pending && <LoadState state="loading" title="Answering suggestion" />}
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
        className="max-h-[33vh] space-y-2 overflow-y-auto overscroll-contain py-2"
      >
        {suggestions.map((suggestion) => (
          <SuggestionCard key={suggestion.id} suggestion={suggestion} />
        ))}
      </div>
    </details>
  );
}
