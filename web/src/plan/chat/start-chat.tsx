import { useAtomValue, useSetAtom, useStore } from "jotai";
import { SquarePen } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import type { SessionView } from "@/plan/api/schema-chat";
import { authorBlockReasonAtom, canAuthorAtom, serverInfoAtom } from "@/plan/state/atoms";
import { BusyText } from "@/plan/ui/busy-text";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { LoadState } from "@/plan/ui/load-state";
import type { Store } from "@/state/store";
import { briefOpenAtom } from "./brief-dialog";
import { focusComposerAtom, composeAfterAtom } from "./composer-hooks";
import { useCanEndChat } from "./end-chat";
import { sessionAtom } from "./model";
import { offerErrorAtom } from "./offer-error";
import { confirmEndFirst, elsewhereRefusal, endCurrentChat, startError } from "./offer-flow";
import { elsewhereSessionAtom, isActiveSession } from "./session-home";
import { revealChat } from "./sheet-state";
import { clearedChatAtom, shownSessionAtom } from "./shown-session";
import { useSessionBrief } from "./use-session-brief";
import { Untrusted, visibleControls } from "./visible-controls";

// A live chat is ended first (the user confirmed in `useNewChat`). New chat then starts nothing: it
// clears the concluded chat from the panel and focuses an empty composer, and the first Send starts
// the session (`useSendDraft`). The head draws New chat only for a live chat; a chat started by
// another tab between render and click reaches the end-then-clear branch. start-chat-brief.test
// drives it.
async function beginChat(store: Store, signal: AbortSignal): Promise<void> {
  const current = store.get(sessionAtom);
  if (isActiveSession(current)) await endCurrentChat(store, current, signal, "start a new chat");
  if (signal.aborted) return;
  store.set(clearedChatAtom, store.get(shownSessionAtom)?.id ?? null);
  store.set(composeAfterAtom, null);
  store.set(focusComposerAtom, true);
  revealChat(store);
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
    if (isActiveSession(store.get(sessionAtom)) && !confirmEndFirst(store)) return;
    const controller = new AbortController();
    request.current = controller;
    setPending(true);
    setError(null);
    try {
      await beginChat(store, controller.signal);
    } catch (cause) {
      if (!controller.signal.aborted) setError(startError(store, cause));
    } finally {
      request.current = null;
      if (!controller.signal.aborted) setPending(false);
    }
  };
  return { start, pending, error };
}

/**
 * `head`: an icon-only button in the panel head, drawn only while this browser may end the live
 * chat; its name is "New chat". `concluded`: the labelled primary button beside an ended chat.
 */
export function NewChatButton({ variant }: { variant: "head" | "concluded" }) {
  const canAuthor = useAtomValue(canAuthorAtom);
  const canEnd = useCanEndChat();
  const reason = useAtomValue(authorBlockReasonAtom);
  const server = useAtomValue(serverInfoAtom);
  const { start, pending, error } = useNewChat();
  // A rejected end-then-wait may leave no live chat; its error must still show.
  if (variant === "head" && !canEnd && !error) return null;
  if ((!canAuthor && !error) || (server?.read_only.length ?? 0) > 0) return null;
  const head = variant === "head";
  return (
    <div className="chat-new-chat text-[13px]" data-variant={variant}>
      {canAuthor && (
        <DisabledReason
          reason={reason ?? (pending ? "Starting a new chat" : null)}
          onClick={() => {
            void start();
          }}
          size="sm"
          shape={head ? "round" : "pill"}
          look={head ? "quiet" : "primary"}
        >
          {head ? (
            <>
              <SquarePen size={15} aria-hidden="true" />
              <span className="sr-only">New chat</span>
            </>
          ) : (
            "Start a new chat"
          )}
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
              <BusyText>Loading brief…</BusyText>
            </p>
          )}
          {state.status === "unavailable" && (
            <LoadState state="error" size="compact" title="Brief not available" />
          )}
          {state.status === "error" && (
            <LoadState
              state="error"
              size="compact"
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
