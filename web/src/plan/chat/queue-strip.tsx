import { useAtomValue, useStore } from "jotai";
import { useState } from "react";

import type { QueuedMessage } from "@/plan/api/schema-chat";
import { authorBlockReasonAtom, isHolderAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { withdrawMessage } from "./commands";
import { draftAtom } from "./drafts";
import { queueAtom } from "./model";
import { Untrusted } from "./visible-controls";

function useWithdraw() {
  const store = useStore();
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  const withdraw = (item: QueuedMessage, edit: boolean) => {
    setError(null);
    setPending(item.id);
    void withdrawMessage(store, item.id)
      .then(() => {
        const text = item.text;
        if (edit && text !== null) {
          store.set(draftAtom, (draft) => ({
            ...draft,
            text: [draft.text, text].filter((part) => part.length > 0).join("\n\n"),
          }));
        }
      })
      .catch((err: unknown) => setError(err instanceof Error ? err.message : String(err)))
      .finally(() => setPending(null));
  };
  return { withdraw, error, pending };
}

function QueueRow({
  item,
  reason,
  pending,
  withdraw,
}: {
  item: QueuedMessage;
  reason: string | null;
  pending: string | null;
  withdraw: (item: QueuedMessage, edit: boolean) => void;
}) {
  const busy = pending ? "A queue change is pending" : null;
  return (
    <li className="flex items-center gap-2 text-xs">
      <span className="min-w-0 flex-1 truncate">
        <Untrusted text={item.first_line} />
      </span>
      <DisabledReason reason={reason ?? busy} onClick={() => withdraw(item, false)} size="sm">
        Remove
      </DisabledReason>
      <DisabledReason
        reason={reason ?? busy ?? (item.text === null ? "Message text is unavailable" : null)}
        onClick={() => withdraw(item, true)}
        size="sm"
      >
        Edit
      </DisabledReason>
    </li>
  );
}

export function QueueStrip() {
  const queue = useAtomValue(queueAtom);
  const isHolder = useAtomValue(isHolderAtom);
  const reason = useAtomValue(authorBlockReasonAtom);
  const { withdraw, error, pending } = useWithdraw();
  if (!queue.length) return null;

  const row = (item: QueuedMessage) => (
    <QueueRow key={item.id} item={item} reason={reason} pending={pending} withdraw={withdraw} />
  );
  return (
    <section aria-label="Queued messages" className="chat-ghost-actions text-xs">
      <p>Queued ({queue.length})</p>
      {isHolder && (
        <>
          <ul>{queue.slice(0, 2).map(row)}</ul>
          {queue.length > 2 && (
            <details>
              <summary>+{queue.length - 2} more</summary>
              <ul>{queue.slice(2).map(row)}</ul>
            </details>
          )}
          {error && (
            <p role="status" className="text-muted-foreground">
              <Untrusted text={error} />
            </p>
          )}
        </>
      )}
    </section>
  );
}
