import { useAtomValue, useStore } from "jotai";
import { Coins } from "lucide-react";
import { useState } from "react";
import { Link } from "react-router";

import { compactTokens } from "@/lib/tokens";
import type { DraftTeamView } from "@/plan/api/schema-kickoff";
import { endChat } from "@/plan/chat/commands";
import { sessionAtom } from "@/plan/chat/model";
import { isActiveSession } from "@/plan/chat/session-home";
import { Untrusted } from "@/plan/chat/visible-controls";
import { plural } from "@/plan/ui/plural";
import { TeammateCard, totalTokens } from "@/plan/feature/team-card";
import { teamDocument, teamWords, type TeamWords } from "@/plan/feature/team-model";
import { useActiveTeam } from "@/plan/feature/use-active-team";
import { docRel, toAddress } from "@/plan/files/address";
import { authorBlockReasonAtom, canAuthorAtom } from "@/plan/state/atoms";
import { StateMark } from "@/plan/ui/activity-glyphs";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { DocMark } from "@/plan/ui/doc-mark";

import "./team.css";

function DocumentRow({
  path,
  team,
  words,
}: {
  path: string;
  team: DraftTeamView;
  words: TeamWords;
}) {
  const document = teamDocument(team, path);
  return (
    <li className="plan-team__doc">
      <DocMark kind={docRel(path).startsWith("plans/") ? "plan" : "section"} />
      <Link to={toAddress(path)} className="min-w-0 break-all font-mono text-[13px] underline">
        <Untrusted text={path} />
      </Link>
      {document?.owner != null && (
        <span className="text-sm text-muted-foreground">
          <Untrusted text={document.owner} />
        </span>
      )}
      <span className="inline-flex items-center gap-1 text-sm">
        {document?.state === "Drafting" && <StateMark kind="drafting" />}
        {words.state[document?.state ?? "Queued"]}
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

function TeamCost({ team, finished }: { team: DraftTeamView; finished: boolean }) {
  const tokens = team.teammates.reduce((sum, teammate) => sum + totalTokens(teammate.tokens), 0);
  return (
    <div className="plan-team-cost">
      <Coins className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
      <p>
        {plural(team.teammates.length, "teammate")} · {plural(team.documents.length, "document")} ·{" "}
        {compactTokens(tokens)} tokens{finished ? "" : " so far"}
      </p>
      {!finished && (
        <div className="plan-team-cost__stop">
          <StopTeam key={team.session} sessionId={team.session} />
        </div>
      )}
    </div>
  );
}

/**
 * The running team. `documents` draws the per-document list; a page whose section queue already
 * lists those documents turns it off, so each document is listed once.
 */
export function DraftTeamPanel({
  feature,
  documents = true,
}: {
  feature: string;
  documents?: boolean;
}) {
  const active = useActiveTeam(feature);
  const session = useAtomValue(sessionAtom);
  if (active === null) return null;
  const { team, finished } = active;
  const words = teamWords(session?.offer);
  // PaceChoice and checklist gaps own draft offers; this panel reports the running team.
  return (
    <section aria-label={words.region} className="plan-team">
      <h2 className="plan-team__title">{finished ? words.finished : words.running}</h2>
      <TeamCost team={team} finished={finished} />
      {documents && (
        <ol aria-label="Document queue" className="plan-team__docs">
          {team.documents.map((path) => (
            <DocumentRow key={path} path={path} team={team} words={words} />
          ))}
        </ol>
      )}
      <h3 className="plan-team__sub">Teammates</h3>
      <ul aria-label="Teammates" className="plan-team__cards">
        {team.teammates.map((teammate) => (
          <TeammateCard key={teammate.agent_id} teammate={teammate} />
        ))}
      </ul>
    </section>
  );
}
