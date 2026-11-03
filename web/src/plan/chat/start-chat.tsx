import { useAtomValue, useSetAtom, useStore } from "jotai";
import { useEffect, useRef, useState } from "react";

import type { SessionView } from "@/plan/api/schema-chat";
import { authorBlockReasonAtom, canAuthorAtom, serverInfoAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { LoadState } from "@/plan/ui/load-state";
import type { Store } from "@/state/store";
import { briefOpenAtom } from "./brief-dialog";
import { startChat } from "./commands";
import { sessionAtom } from "./model";
import { offerErrorAtom } from "./offer-error";
import { confirmEndFirst, elsewhereRefusal, endCurrentChat, startError } from "./offer-flow";
import { panelCollapsedAtom } from "./prefs";
import {
  elsewhereSessionAtom,
  isActiveSession,
  startOwnerAtom,
  type StartOwner,
} from "./session-home";
import { chatSheetOpenAtom } from "./sheet-state";
import { useSessionBrief } from "./use-session-brief";
import { Untrusted, visibleControls } from "./visible-controls";

// New chat renders only without an active session, but a chat started by another tab between render
// and click reaches the end-then-start branch; it stays as the guard for that race.
// start-chat-brief.test drives it.
async function beginChat(store: Store, owner: StartOwner, signal: AbortSignal): Promise<void> {
  const current = store.get(sessionAtom);
  if (isActiveSession(current)) await endCurrentChat(store, current, signal, "start a new chat");
  if (signal.aborted) return;
  await startChat(store, null, null, [], owner.feature, owner.plan);
  store.set(panelCollapsedAtom, false);
  store.set(chatSheetOpenAtom, true);
}

/** Runs one New chat request at a time; unmounting aborts the wait for the old chat to end. */
function useNewChat() {
  const store = useStore();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const request = useRef<AbortController | null>(null);
  useEffect(() => () => request.current?.abort(), []);
  const start = async () => {
    if (request.current) return;
    const elsewhere = store.get(elsewhereSessionAtom);
    if (elsewhere) {
      store.set(offerErrorAtom, elsewhereRefusal(elsewhere));
      return;
    }
    if (isActiveSession(store.get(sessionAtom)) && !confirmEndFirst()) return;
    // Read at the click: ending the running chat can take minutes, and the user may navigate.
    const owner = store.get(startOwnerAtom);
    const controller = new AbortController();
    request.current = controller;
    setPending(true);
    setError(null);
    try {
      await beginChat(store, owner, controller.signal);
    } catch (cause) {
      if (!controller.signal.aborted) setError(startError(store, cause));
    } finally {
      request.current = null;
      if (!controller.signal.aborted) setPending(false);
    }
  };
  return { start, pending, error };
}

export function NewChatButton() {
  const canAuthor = useAtomValue(canAuthorAtom);
  const reason = useAtomValue(authorBlockReasonAtom);
  const server = useAtomValue(serverInfoAtom);
  const { start, pending, error } = useNewChat();
  if ((!canAuthor && !error) || (server?.read_only.length ?? 0) > 0) return null;
  return (
    <div className="text-[13px]">
      {canAuthor && (
        <DisabledReason
          reason={reason ?? (pending ? "Starting a new chat" : null)}
          onClick={() => {
            void start();
          }}
          size="sm"
        >
          New chat
        </DisabledReason>
      )}
      {error && (
        <p role="status">
          <Untrusted text={error} />
        </p>
      )}
    </div>
  );
}

/**
 * The Starting row: name, model and effort from the brief, and a link to read the brief itself.
 * The dialog is rendered by `BriefDialogHost`, which outlives this row.
 */
export function StartingBrief({ session }: { session: SessionView }) {
  const starting = session.state === "starting";
  // Only an authoring session has a brief; a pressure test's start must not read as a failure.
  const hasBrief = session.kind === "authoring";
  const state = useSessionBrief(starting && hasBrief ? session.id : null);
  const openBrief = useSetAtom(briefOpenAtom);
  if (!starting) return null;
  const brief = state.status === "ready" ? state.brief : null;
  return (
    <div className="space-y-2 text-[13px]">
      <p>
        Starting session · <Untrusted text={brief?.display_name ?? session.name} /> ·{" "}
        <Untrusted text={brief?.model ?? session.model} />
        {brief && (
          <>
            {" "}
            · <Untrusted text={brief.effort} />
          </>
        )}
      </p>
      {hasBrief && (
        <>
          {state.status === "loading" && (
            <p role="status" className="text-xs text-muted-foreground">
              Loading brief…
            </p>
          )}
          {state.status === "unavailable" && (
            <LoadState state="error" title="Brief not available" />
          )}
          {state.status === "error" && (
            <LoadState
              state="error"
              title="Brief not available"
              message={visibleControls(state.message)}
            />
          )}
          <button type="button" className="underline" onClick={() => openBrief(session.id)}>
            Brief
          </button>
        </>
      )}
    </div>
  );
}
