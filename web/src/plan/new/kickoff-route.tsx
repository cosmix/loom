import { useAtomValue, useStore } from "jotai";
import { useLocation } from "react-router";

import type { KickoffView } from "@/plan/api/schema-kickoff";
import { ComposerArea } from "@/plan/chat/composer";
import { sessionAtom, useChatSocket } from "@/plan/chat/model";
import { isActiveSession } from "@/plan/chat/session-home";
import { Transcript } from "@/plan/chat/transcript";
import { useFileSocket } from "@/plan/files/api";
import { EmptyKickoff } from "@/plan/new/empty-kickoff";
import { KickoffHead } from "@/plan/new/kickoff-head";
import { useKickoffLanding } from "@/plan/new/kickoff-landing";
import { isKickoffSession, kickoffSessionAtom } from "@/plan/new/kickoff-state";
import { Proposal } from "@/plan/new/proposal";
import { TakingShape } from "@/plan/new/taking-shape";
import { kickoffAtom } from "@/plan/state/atoms";
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
    <div className="kickoff-grid">
      <section aria-label="Kickoff conversation" className="kickoff-transcript">
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
