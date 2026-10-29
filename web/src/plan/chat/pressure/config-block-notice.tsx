import { useAtomValue, useStore } from "jotai";
import { useState } from "react";

import type { ConfigBlock } from "@/plan/api/schema-pressure";
import { canAuthorAtom, serverInfoAtom } from "@/plan/state/atoms";
import { ActionError } from "@/plan/ui/action-error";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { Untrusted } from "../visible-controls";
import { confirmConfig, refreshPressureOptions, usePressureOptions } from "./use-pressure";

const NOTICE_NAME = "Codex configuration change";

/**
 * Section 12, Accepted limits: a Codex change to the session configuration blocks every
 * planning session until a browser confirms the files. The block is persisted, so this reads
 * the server's options instead of a run view, which a server restart loses. Every browser sees
 * the paths (or the reason, when it names none); a browser that holds authoring, or finds it
 * free, can confirm (the server takes it for the confirmation). Like Stop, the control is absent
 * for any other browser. `context` names where the notice is mounted, so two notices on one page (the chat dock and
 * the plan's actions) are distinct landmarks.
 */
export function ConfigBlockNotice({ context }: { context?: string }) {
  const options = usePressureOptions();
  const block = options?.ok ? options.value.config_block : null;
  return block ? <BlockNotice block={block} context={context} /> : null;
}

function BlockNotice({ block, context }: { block: ConfigBlock; context?: string }) {
  const { mayConfirm, pending, error, confirm } = useConfirmConfig();
  return (
    <section
      aria-label={context ? `${NOTICE_NAME} (${context})` : NOTICE_NAME}
      className="space-y-1 text-xs"
    >
      <BlockSummary block={block} />
      {mayConfirm && (
        <DisabledReason
          reason={pending ? "Confirming…" : null}
          onClick={() => {
            void confirm();
          }}
          size="sm"
        >
          I have checked these files
        </DisabledReason>
      )}
      {error && <ActionError message={error} />}
    </section>
  );
}

// The server builds the reason as a fixed sentence followed by every changed path, so with
// paths listed the reason would repeat them outside the bounded list; only a reason that
// names no files (Codex did not exit, the files were unreadable) is shown as written.
function BlockSummary({ block }: { block: ConfigBlock }) {
  if (block.changed.length === 0) {
    return (
      <p className="whitespace-pre-wrap [overflow-wrap:anywhere]">
        <Untrusted text={block.reason} />
      </p>
    );
  }
  return (
    <>
      <p>Codex changed these session configuration files:</p>
      <ChangedPaths paths={block.changed} />
    </>
  );
}

function ChangedPaths({ paths }: { paths: string[] }) {
  // Bounded so a long list cannot push the transcript away; focusable so it scrolls by keyboard.
  return (
    <ul
      tabIndex={0}
      aria-label="Changed files"
      className="max-h-40 overflow-auto font-mono [overflow-wrap:anywhere] focus-visible:outline-2 focus-visible:outline-ring"
    >
      {paths.map((path) => (
        <li key={path}>
          <Untrusted text={path} />
        </li>
      ))}
    </ul>
  );
}

function useConfirmConfig() {
  const store = useStore();
  const canAuthor = useAtomValue(canAuthorAtom);
  const server = useAtomValue(serverInfoAtom);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // canAuthorAtom admits the holder even in read-only mode, which the server refuses.
  const mayConfirm = canAuthor && server?.read_only.length === 0;

  async function confirm() {
    if (pending || !store.get(canAuthorAtom)) return;
    if (store.get(serverInfoAtom)?.read_only.length !== 0) return;
    setPending(true);
    setError(null);
    const result = await confirmConfig();
    // A refused confirmation (someone else cleared the block first) is also settled by the
    // server's current options, so the notice never outlives the block.
    await refreshPressureOptions(store);
    setPending(false);
    if (!result.ok) setError(result.message);
  }

  return { mayConfirm, pending, error, confirm };
}
