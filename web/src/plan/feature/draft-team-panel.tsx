import { useAtomValue, useStore } from "jotai";
import { Coins } from "lucide-react";
import { useState } from "react";
import { Link } from "react-router";

import { compactTokens } from "@/lib/tokens";
import type { DraftTeamView, TeammateStatus } from "@/plan/api/schema-kickoff";
import { endChat } from "@/plan/chat/commands";
import { sessionAtom } from "@/plan/chat/model";
import { isActiveSession } from "@/plan/chat/session-home";
import { Untrusted } from "@/plan/chat/visible-controls";
import { plural } from "@/plan/feature/steps";
import { TeammateCard, totalTokens } from "@/plan/feature/team-card";
import { docRel, toAddress } from "@/plan/files/address";
import {
  authorBlockReasonAtom,
  canAuthorAtom,
  draftTeamAtom,
  featuresAtom,
} from "@/plan/state/atoms";
import { ActivityGlyph } from "@/plan/ui/activity-glyphs";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { DocMark } from "@/plan/ui/doc-mark";

import "./team.css";

function documentState(path: string, teammate: TeammateStatus | undefined) {
  if (!teammate) return "Queued";
  if (teammate.state === "working" && teammate.current === path) return "Drafting";
  if (teammate.state === "idle" && teammate.current === null) return "Drafted";
  const currentIndex =
    teammate.current === null ? -1 : teammate.documents.indexOf(teammate.current);
  return teammate.documents.indexOf(path) < currentIndex ? "Drafted" : "Queued";
}

function DocumentRow({ path, teammates }: { path: string; teammates: TeammateStatus[] }) {
  const teammate = teammates.find((candidate) => candidate.documents.includes(path));
  const state = documentState(path, teammate);
  return (
    <li className="plan-team__doc">
      <DocMark kind={docRel(path).startsWith("plans/") ? "plan" : "section"} />
      <Link to={toAddress(path)} className="min-w-0 break-all font-mono text-[13px] underline">
        <Untrusted text={path} />
      </Link>
      <span className="text-sm text-muted-foreground">
        {teammate ? <Untrusted text={teammate.name} /> : "Unassigned"}
      </span>
      <span className="inline-flex items-center gap-1 text-sm">
        {state === "Drafting" && <ActivityGlyph kind="drafting" />}
        {state}
      </span>
    </li>
  );
}

function StopTeam({ sessionId }: { sessionId: string }) {
  const store = useStore();
  const canAuthor = useAtomValue(canAuthorAtom);
  const blockReason = useAtomValue(authorBlockReasonAtom);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const canStop = () => {
    const session = store.get(sessionAtom);
    return store.get(canAuthorAtom) && isActiveSession(session) && session.id === sessionId;
  };
  const stop = () => {
    if (!canStop() || pending) return;
    if (!window.confirm("Stop the team? Documents already written stay.")) return;
    if (!canStop()) return;
    setPending(true);
    setError(null);
    void endChat(store)
      .catch((error: unknown) => setError(error instanceof Error ? error.message : String(error)))
      .finally(() => setPending(false));
  };
  const reason = !canAuthor
    ? "Stopping the team requires authoring"
    : pending
      ? "Stopping the team"
      : null;
  return (
    <div className="space-y-1">
      <DisabledReason reason={reason} onClick={stop} size="sm">
        Stop the team
      </DisabledReason>
      {!canAuthor && blockReason && (
        <p className="text-xs text-muted-foreground">
          <Untrusted text={blockReason} />
        </p>
      )}
      {error && (
        <p role="alert" className="text-sm">
          <Untrusted text={error} />
        </p>
      )}
    </div>
  );
}

function TeamCost({ team }: { team: DraftTeamView }) {
  const tokens = team.teammates.reduce((sum, teammate) => sum + totalTokens(teammate.tokens), 0);
  return (
    <div className="plan-team-cost">
      <Coins className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
      <p>
        {plural(team.teammates.length, "teammate")} · {plural(team.documents.length, "document")} ·{" "}
        {compactTokens(tokens)} tokens so far
      </p>
      <div className="plan-team-cost__stop">
        <StopTeam key={team.session} sessionId={team.session} />
      </div>
    </div>
  );
}

export function DraftTeamPanel({ feature }: { feature: string }) {
  const team = useAtomValue(draftTeamAtom);
  const session = useAtomValue(sessionAtom);
  const features = useAtomValue(featuresAtom);
  if (team === null) return null;
  const activeSession = isActiveSession(session) && team.session === session.id;
  const offeredBy = features.filter((checklist) =>
    [checklist.primary, ...checklist.secondary, ...checklist.items.map((item) => item.offer)].some(
      (offer) => session?.offer != null && offer?.id === session.offer,
    ),
  );
  // Offer IDs can be shared across features; an ambiguous offer cannot identify the owner.
  const inFeature =
    team.documents.length > 0
      ? team.documents.some(
          (path) =>
            path.startsWith(`doc/product/${feature}/`) || path.startsWith(`doc/plans/${feature}/`),
        )
      : offeredBy.length === 1 && offeredBy[0]?.feature === feature;
  if (!activeSession || !inFeature) return null;
  // PaceChoice and checklist gaps own draft offers; this panel reports the running team.
  return (
    <section aria-label="Draft team" className="plan-team">
      <h2 className="plan-team__title">Drafting with a team</h2>
      <TeamCost team={team} />
      <ol aria-label="Document queue" className="plan-team__docs">
        {team.documents.map((path) => (
          <DocumentRow key={path} path={path} teammates={team.teammates} />
        ))}
      </ol>
      <h3 className="plan-team__sub">Teammates</h3>
      <ul aria-label="Teammates" className="plan-team__cards">
        {team.teammates.map((teammate) => (
          <TeammateCard key={teammate.agent_id} teammate={teammate} />
        ))}
      </ul>
    </section>
  );
}
