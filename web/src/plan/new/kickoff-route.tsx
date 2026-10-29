import { useAtomValue, useStore } from "jotai";
import { useRef, useState } from "react";
import { useLocation } from "react-router";

import type { KickoffSize, KickoffView } from "@/plan/api/schema-kickoff";
import { sendMessage } from "@/plan/chat/commands";
import { ComposerArea } from "@/plan/chat/composer";
import { sessionAtom, useChatSocket } from "@/plan/chat/model";
import { isActiveSession } from "@/plan/chat/offer-flow";
import { Transcript } from "@/plan/chat/transcript";
import { Untrusted } from "@/plan/chat/visible-controls";
import { useFileSocket } from "@/plan/files/api";
import { EmptyKickoff } from "@/plan/new/empty-kickoff";
import { useKickoffLanding } from "@/plan/new/kickoff-landing";
import { isKickoffSession, kickoffSessionAtom } from "@/plan/new/kickoff-state";
import { Proposal } from "@/plan/new/proposal";
import { TakingShape } from "@/plan/new/taking-shape";
import { authorBlockReasonAtom, canAuthorAtom, kickoffAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import "@/plan/new/kickoff.css";

export function KickoffRoute() {
  const store = useStore();
  const location = useLocation();
  const session = useAtomValue(sessionAtom);
  const tab = useAtomValue(kickoffSessionAtom);
  const view = useAtomValue(kickoffAtom);
  const current = view && session && view.session === session.id ? view : null;
  const active = isActiveSession(session) && isKickoffSession(session.id, tab, current);
  // ChatPanel normally subscribes; kickoff replaces it while this route is mounted.
  useChatSocket(store);
  useFileSocket(store);
  useKickoffLanding(active ? current : null);
  if (!active) return <EmptyKickoff small={location.state?.small === true} />;
  return (
    <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col lg:flex-row">
      <section
        aria-label="Kickoff conversation"
        className="kickoff-transcript flex min-h-96 min-w-0 flex-1 flex-col lg:min-h-0"
      >
        <Transcript />
        <ComposerArea />
      </section>
      <KickoffSidebar view={current} />
    </div>
  );
}

function KickoffSidebar({ view }: { view: KickoffView | null }) {
  return (
    <div className="min-w-0 shrink-0 space-y-4 border-t border-border p-4 font-sans lg:w-80 lg:overflow-y-auto lg:border-t-0 lg:border-l xl:w-96">
      <TakingShape shape={view?.shape ?? null} target={view?.target ?? null} />
      {view?.target == null ? (
        <>
          {view && <Proposal shape={view.shape} />}
          <p className="text-sm text-muted-foreground">
            Nothing is written under doc/ until you say so.
          </p>
        </>
      ) : (
        view.shape.size && <WriteKickoff size={view.shape.size} />
      )}
    </div>
  );
}

function useKickoffWrite(size: KickoffSize) {
  const store = useStore();
  const canAuthor = useAtomValue(canAuthorAtom);
  const authorReason = useAtomValue(authorBlockReasonAtom);
  const session = useAtomValue(sessionAtom);
  const inFlight = useRef(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const label = size === "feature" ? "Write the vision" : "Write the plan";
  const reason = !canAuthor
    ? "Writing requires authoring"
    : session?.state !== "idle"
      ? "Wait for the current turn to finish"
      : pending
        ? "Sending the write request"
        : null;
  const write = async () => {
    if (reason !== null || !store.get(canAuthorAtom) || inFlight.current) return;
    inFlight.current = true;
    setPending(true);
    setError(null);
    try {
      await sendMessage(store, `${label}.`);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      inFlight.current = false;
      setPending(false);
    }
  };
  return { label, reason, authorReason: canAuthor ? null : authorReason, error, write };
}

function WriteKickoff({ size }: { size: KickoffSize }) {
  const { label, reason, authorReason, error, write } = useKickoffWrite(size);
  return (
    <div className="space-y-2">
      <DisabledReason reason={reason} onClick={() => void write()}>
        {label}
      </DisabledReason>
      {authorReason !== null && (
        <p className="text-xs text-muted-foreground">
          <Untrusted text={authorReason} />
        </p>
      )}
      {error !== null && (
        <p role="status">
          <Untrusted text={error} />
        </p>
      )}
    </div>
  );
}
