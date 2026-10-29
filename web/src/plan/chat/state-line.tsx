import { useAtomValue } from "jotai/react";

import { BusyRoundel } from "@/aurora-ui/feedback/BusyRoundel";
import { HazardPanel } from "@/aurora-ui/feedback/HazardPanel";
import { WorkRoundel } from "@/components/activity-roundel";
import { useNow } from "@/components/hooks/use-now";
import { formatElapsed } from "@/lib/format";
import type { ServerInfo } from "@/plan/api/schema";
import type { Item, SessionView, Turn } from "@/plan/api/schema-chat";
import { serverInfoAtom } from "@/plan/state/atoms";
import { pendingPermissionsAtom, turnsAtom } from "./model";
import { NewChatOffers } from "./new-chat-offers";
import { shownSessionAtom } from "./shown-session";
import { StartingBrief } from "./start-chat";
import { Untrusted } from "./visible-controls";

type Permission = Extract<Item, { type: "permission" }>;

function age(since: string, now: number): string {
  const seconds = Math.floor((now - Date.parse(since)) / 1000);
  return formatElapsed(Number.isFinite(seconds) ? Math.max(0, seconds) : 0);
}

/** Working-row extras that change every few seconds: shown on screen, never announced. */
function workingDetail(session: SessionView, turns: Turn[], now: number): string[] {
  const turn = turns.findLast((candidate) => candidate.open);
  const since = session.working_since ?? turn?.started_at ?? session.state_since;
  const tool = turn?.items.findLast((item) => item.type === "tool");
  const parts = [age(since, now)];
  if (tool?.type === "tool") parts.push(tool.summary);
  if (session.subagents_running > 0) {
    parts.push(
      `${session.subagents_running} subagent${session.subagents_running === 1 ? "" : "s"}`,
    );
  }
  return parts;
}

/**
 * The words on the row, and what the polite live region announces: the state and its stable
 * facts only. Clocks and tool summaries stay out, or the region would re-announce every few
 * seconds for the whole turn.
 */
function stateWords(session: SessionView | null, pending: Permission[]): string {
  if (!session) return "No chat";
  switch (session.state) {
    case "starting":
      return `Starting session · ${session.name} · ${session.model}`;
    case "idle":
      return "Waiting for you";
    case "working":
      return "Working";
    case "permission":
      return pending.length > 1
        ? `Needs ${pending.length} decisions: ${pending.map((item) => item.tool).join(", ")}`
        : `Needs a decision${pending[0] ? `: ${pending[0].tool}` : ""}`;
    case "ending":
      return "Writing final notes into the documents · This can take up to 10 minutes.";
    case "ended":
    case "failed":
      // The Ended line and the Failure panel carry these on screen; ChatAnnouncer says them aloud.
      return "";
  }
}

/**
 * States whose announcement belongs to ChatAnnouncer, which stays mounted when the panel does
 * not (a collapsed rail, a closed sheet). The polite region here leaves them out, so each is
 * announced once, and the permission words on the row are not hidden behind a duplicate.
 */
function announcedElsewhere(session: SessionView | null): boolean {
  const state = session?.state;
  return state === "permission" || state === "ended" || state === "failed";
}

function StateGlyph({ state }: { state: SessionView["state"] | undefined }) {
  if (state === "working") {
    // WorkRoundel currently reads dashboard stages. Override only its animation
    // until its owner exposes a session activity input; reduced motion still wins.
    return (
      <span
        aria-hidden="true"
        className="[&_.aurora-roundel]:animate-[aurora-roundel-spin_1.6s_linear_infinite] motion-reduce:[&_.aurora-roundel]:animate-none"
      >
        <WorkRoundel size={14} />
      </span>
    );
  }
  if (state === "permission") {
    return (
      <span aria-hidden="true" className="text-(--tone-warning)">
        ⚠
      </span>
    );
  }
  return (
    <span aria-hidden="true">
      <BusyRoundel size={14} busy={state === "starting" || state === "ending"} />
    </span>
  );
}

function Failure({ session, now }: { session: SessionView; now: number }) {
  const title =
    session.failure?.kind === "spawn"
      ? "The session could not start"
      : "The session stopped unexpectedly";
  return (
    <HazardPanel tone="error" title={title}>
      {session.failure && (
        <p>
          <Untrusted text={session.failure.message} />
        </p>
      )}
      {session.failure?.kind !== "spawn" && session.last_activity && (
        <p>
          Last activity{" "}
          <time dateTime={session.last_activity}>
            <Untrusted text={session.last_activity} /> ({age(session.last_activity, now)} ago)
          </time>
        </p>
      )}
      <p>The documents keep what the session recorded as it went. A new chat starts fresh.</p>
    </HazardPanel>
  );
}

function ReadOnlyHazard({ problems }: { problems: ServerInfo["read_only"] }) {
  return (
    <HazardPanel tone="warning" title="Read-only">
      <ul>
        {problems.map((problem, index) => (
          <li key={`${problem.what}-${index}`}>
            <p>
              <Untrusted text={problem.message} />
            </p>
            <p>
              <Untrusted text={problem.fix} />
            </p>
          </li>
        ))}
      </ul>
    </HazardPanel>
  );
}

/**
 * `hideWords` hides the state words from browse mode (the sr-only region announces them);
 * `detail` (elapsed time, newest tool, subagents) is not announced there, so it stays exposed.
 */
function StateRow(props: {
  session: SessionView | null;
  text: string;
  detail: string;
  hideWords: boolean;
}) {
  const { session, text, detail, hideWords } = props;
  return (
    <div className="flex flex-wrap items-center gap-2">
      <StateGlyph state={session?.state} />
      {session?.state === "starting" ? (
        <StartingBrief session={session} />
      ) : (
        <span>
          <span aria-hidden={hideWords || undefined}>
            <Untrusted text={text} />
          </span>
          {detail && <Untrusted text={detail} />}
        </span>
      )}
      {!session && <NewChatOffers />}
    </div>
  );
}

function StateBody(props: {
  session: SessionView | null;
  problems: ServerInfo["read_only"];
  text: string;
  detail: string;
  hideWords: boolean;
  now: number;
}) {
  const { session, problems, text, detail, hideWords, now } = props;
  if (problems.length > 0) return <ReadOnlyHazard problems={problems} />;
  if (session?.state === "failed") return <Failure session={session} now={now} />;
  if (session?.state === "ended") {
    // Plain text, exposed to browse mode but not a live region: ChatAnnouncer says it aloud once.
    const ago = age(session.state_since, now);
    return <p className="text-muted-foreground">{`Chat ended · ${ago} ago`}</p>;
  }
  return <StateRow session={session} text={text} detail={detail} hideWords={hideWords} />;
}

export function StateLine() {
  const session = useAtomValue(shownSessionAtom);
  const turns = useAtomValue(turnsAtom);
  const pending = useAtomValue(pendingPermissionsAtom);
  const problems = useAtomValue(serverInfoAtom)?.read_only ?? [];
  const now = useNow();
  const readOnly = problems.length > 0;
  const words = readOnly
    ? `Read-only · ${problems.map((p) => `${p.message}. ${p.fix}`).join(" · ")}`
    : stateWords(session, pending);
  const detail =
    session?.state === "working" && !readOnly
      ? ` · ${workingDetail(session, turns, now).join(" · ")}`
      : "";
  // The sr-only region announces the words, so the visible copy is hidden from browse mode on the
  // rows that hold only those words (and buttons beside them); read-only and Starting rows carry
  // more than the announcement and stay exposed.
  const elsewhere = !readOnly && announcedElsewhere(session);
  const hideWords = !readOnly && !elsewhere && session?.state !== "starting";
  return (
    <div className="chat-state-line px-3 text-[13px] font-medium">
      <div role="status" aria-live="polite" aria-atomic="true" className="sr-only">
        <Untrusted text={elsewhere ? "" : words} />
      </div>
      <div aria-live="off">
        <StateBody
          session={session}
          problems={problems}
          text={words}
          detail={detail}
          hideWords={hideWords}
          now={now}
        />
      </div>
    </div>
  );
}
