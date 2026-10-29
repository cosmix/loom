import { useAtomValue, useStore } from "jotai";
import { useState } from "react";
import { Link } from "react-router";

import { compactTokens } from "@/lib/tokens";
import type { DraftTeamView, TeammateStatus, TokenCounts } from "@/plan/api/schema-kickoff";
import { endChat } from "@/plan/chat/commands";
import { sessionAtom } from "@/plan/chat/model";
import { isActiveSession } from "@/plan/chat/offer-flow";
import { Untrusted } from "@/plan/chat/visible-controls";
import { docRel, toAddress } from "@/plan/files/address";
import { authorBlockReasonAtom, canAuthorAtom, draftTeamAtom } from "@/plan/state/atoms";
import { ActivityGlyph } from "@/plan/ui/activity-glyphs";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { DocMark } from "@/plan/ui/doc-mark";

function totalTokens(tokens: TokenCounts): number {
  return tokens.input + tokens.cache_write + tokens.cache_read + tokens.output;
}

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
    <li className="flex flex-wrap items-center gap-x-3 gap-y-1 py-2">
      <DocMark kind={docRel(path).startsWith("plans/") ? "plan" : "section"} />
      <Link to={toAddress(path)} className="min-w-0 break-all font-mono text-sm underline">
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

function TeammateRow({ teammate }: { teammate: TeammateStatus }) {
  const { tokens } = teammate;
  return (
    <li className="space-y-1 py-2 text-sm">
      <p className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <strong className="font-medium">
          <Untrusted text={teammate.name} />
        </strong>
        <span>{teammate.state === "working" ? "working" : "idle"}</span>
        <span>{compactTokens(totalTokens(tokens))} tokens</span>
      </p>
      <p className="break-all text-muted-foreground">
        Current document: {teammate.current ? <Untrusted text={teammate.current} /> : "None"}
      </p>
      <p className="text-muted-foreground">
        {compactTokens(tokens.input)} in / {compactTokens(tokens.output)} out ·{" "}
        {compactTokens(tokens.cache_write)} cache write · {compactTokens(tokens.cache_read)} cache
        read
      </p>
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

function TeamSummary({ team }: { team: DraftTeamView }) {
  const tokens = team.teammates.reduce((sum, teammate) => sum + totalTokens(teammate.tokens), 0);
  return (
    <p className="text-sm text-muted-foreground">
      {team.teammates.length} teammates, {team.documents.length} documents · {compactTokens(tokens)}{" "}
      tokens total
    </p>
  );
}

export function DraftTeamPanel({ feature }: { feature: string }) {
  const team = useAtomValue(draftTeamAtom);
  const session = useAtomValue(sessionAtom);
  if (team === null) return null;
  const activeSession = isActiveSession(session) && team.session === session.id;
  const inFeature = team.documents.some(
    (path) =>
      path.startsWith(`doc/product/${feature}/`) || path.startsWith(`doc/plans/${feature}/`),
  );
  if (!activeSession || (team.documents.length > 0 && !inFeature)) return null;
  return (
    <section aria-label="Draft team" className="space-y-4 border-t border-border py-6 font-sans">
      <h2 className="text-lg font-semibold">Drafting with a team</h2>
      <TeamSummary team={team} />
      <ol aria-label="Document queue" className="divide-y divide-border">
        {team.documents.map((path) => (
          <DocumentRow key={path} path={path} teammates={team.teammates} />
        ))}
      </ol>
      <h3 className="text-sm font-semibold">Teammates</h3>
      <ul aria-label="Teammates" className="divide-y divide-border">
        {team.teammates.map((teammate) => (
          <TeammateRow key={teammate.agent_id} teammate={teammate} />
        ))}
      </ul>
      <StopTeam key={team.session} sessionId={team.session} />
    </section>
  );
}
