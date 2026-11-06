import { useAtomValue, useStore } from "jotai";
import { useEffect } from "react";
import { useLocation, useNavigate, useSearchParams } from "react-router";

import type { KickoffView } from "@/plan/api/schema-kickoff";
import { ComposerArea } from "@/plan/chat/composer";
import { sessionAtom, useChatSocket } from "@/plan/chat/model";
import { isActiveSession } from "@/plan/chat/session-home";
import { revealChat } from "@/plan/chat/sheet-state";
import { Transcript } from "@/plan/chat/transcript";
import { featureHref } from "@/plan/feature/use-feature";
import { useFileSocket } from "@/plan/files/api";
import { DraftView } from "@/plan/new/draft-view";
import { EmptyKickoff } from "@/plan/new/empty-kickoff";
import { KickoffHead } from "@/plan/new/kickoff-head";
import { useKickoffLanding } from "@/plan/new/kickoff-landing";
import { isKickoffSession, kickoffHref, kickoffSessionAtom } from "@/plan/new/kickoff-state";
import { Proposal } from "@/plan/new/proposal";
import { TakingShape } from "@/plan/new/taking-shape";
import { kickoffAtom } from "@/plan/state/atoms";
import "@/plan/new/kickoff.css";

/**
 * `/new` is always the empty form. `/new?session=<id>` is that kickoff: its conversation while the
 * server runs it, its draft once it ended. This tab's own start only navigates here; a follow-up
 * start is a feature chat and moves to that feature's metaplan, where the chat panel shows it.
 */
export function KickoffRoute() {
  const store = useStore();
  const location = useLocation();
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const requested = params.get("session");
  const session = useAtomValue(sessionAtom);
  const tab = useAtomValue(kickoffSessionAtom);
  const view = useAtomValue(kickoffAtom);
  const current = view && session && view.session === session.id ? view : null;
  const active = isActiveSession(session) && isKickoffSession(session.id, requested);
  // ChatPanel normally subscribes; kickoff replaces it while this route is mounted.
  useChatSocket(store);
  useFileSocket(store);
  useKickoffLanding(active ? current : null);
  useEffect(() => {
    // A start from this tab has its session: forget the start, so a later visit to /new stays on
    // the empty form, and move to the kickoff's address.
    if (tab === null || !isActiveSession(session) || session.id !== tab) return;
    store.set(kickoffSessionAtom, null);
    if (requested !== tab) void navigate(kickoffHref(tab), { replace: true });
  }, [tab, session, requested, store, navigate]);
  // Leaving /new before the session arrived must not leave the start to redirect a later visit.
  useEffect(() => () => void store.set(kickoffSessionAtom, null), [store]);
  const onFollowUp = (feature: string) => {
    void navigate(featureHref(feature, "metaplan"));
    revealChat(store);
  };
  if (requested === null) {
    return <EmptyKickoff small={location.state?.small === true} onFollowUp={onFollowUp} />;
  }
  if (!active) return <DraftView id={requested} />;
  return (
    <div className="kickoff-page" data-state="live">
      <section aria-label="Kickoff conversation" className="kickoff-main kickoff-transcript">
        <KickoffHead />
        <Transcript />
        <ComposerArea />
      </section>
      <KickoffSidebar view={current} />
    </div>
  );
}

function KickoffSidebar({ view }: { view: KickoffView | null }) {
  return (
    <div className="kickoff-side">
      <TakingShape shape={view?.shape ?? null} target={view?.target ?? null}>
        {view && view.target == null && <Proposal shape={view.shape} />}
      </TakingShape>
    </div>
  );
}
