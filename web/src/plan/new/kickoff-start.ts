import { useStore } from "jotai";
import { useEffect, useRef, useState } from "react";

import { startChat } from "@/plan/chat/commands";
import { sessionAtom } from "@/plan/chat/model";
import { noPanelRefusal, startError } from "@/plan/chat/offer-flow";
import { isActiveSession } from "@/plan/chat/session-home";
import { kickoffSessionAtom, waitForKickoffSession } from "@/plan/new/kickoff-state";
import { PLAN_FOLLOW_UPS_ID } from "@/plan/new/follow-up-features";
import { canAuthorAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";
import type { StarterDocument } from "./document-starter";

export type WorkKind = "feature" | "plan" | "follow-up";
export type PickedDocument = StarterDocument & { id: string };

type Work = { kind: "feature" | "plan" } | { kind: "follow-up"; feature: string };

/** The work a start asks for, or null for a follow-up with no feature picked. */
function workOf(kind: WorkKind, feature: string | null): Work | null {
  if (kind !== "follow-up") return { kind };
  return feature === null ? null : { kind, feature };
}

const KICKOFF_OFFERS = { feature: "kickoff", plan: "kickoff-small" } as const;

async function beginKickoff(
  store: Store,
  kind: "feature" | "plan",
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
    KICKOFF_OFFERS[kind],
    text,
    documents.map((document) => document.attachment),
  );
  if (!signal.aborted) await waitForKickoffSession(store, previous?.id ?? null, signal, onTimeout);
}

/**
 * A follow-up is a feature chat, not a kickoff: it never sets `kickoffSessionAtom` and waits for no
 * kickoff session, because `KickoffRoute` would move the tab to `/new?session=`.
 */
async function beginWork(
  store: Store,
  work: Work,
  text: string,
  documents: PickedDocument[],
  signal: AbortSignal,
  onTimeout: () => void,
) {
  if (work.kind !== "follow-up") {
    await beginKickoff(store, work.kind, text, documents, signal, onTimeout);
    return;
  }
  const attachments = documents.map((document) => document.attachment);
  await startChat(store, PLAN_FOLLOW_UPS_ID, text, attachments, work.feature);
}

/**
 * Starts a kickoff (`feature`, `plan`) or a Plan follow-ups chat for a picked feature. A follow-up
 * without a feature starts nothing; once its start command resolves `onFollowUp` gets the feature.
 */
export function useKickoffStart(onFollowUp?: (feature: string) => void) {
  const store = useStore();
  const request = useRef<AbortController | null>(null);
  const sessionWait = useRef<AbortController | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => () => sessionWait.current?.abort(), []);
  const start = async (
    kind: WorkKind,
    text: string,
    documents: PickedDocument[],
    feature: string | null,
  ) => {
    const work = workOf(kind, feature);
    if (!work || request.current || !store.get(canAuthorAtom)) return;
    if (!text.trim() && !documents.length) return;
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
      await beginWork(store, work, text, documents, controller.signal, () => {
        if (controller.signal.aborted) return;
        request.current = null;
        setPending(false);
        setError("The conversation did not start. Please try again.");
      });
      if (controller.signal.aborted) return;
      setError(null);
      if (work.kind === "follow-up") onFollowUp?.(work.feature);
    } catch (cause) {
      if (!controller.signal.aborted) setError(startError(store, cause));
    } finally {
      if (request.current === controller) request.current = null;
      if (!controller.signal.aborted) setPending(false);
    }
  };
  return { start, pending, error };
}
