import { useAtomValue } from "jotai/react";
import type { ReactNode } from "react";

import { useNow } from "@/components/hooks/use-now";
import { formatElapsed } from "@/lib/format";
import type { Item, SessionView, Turn } from "@/plan/api/schema-chat";
import { RoundelGlyph } from "@/plan/ui/roundel-glyph";
import { BUSY_STATES } from "./chat-state-glyph";
import { pendingPermissionsAtom, turnsAtom } from "./model";
import { panelSessionAtom } from "./session-home";
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
 * not (a closed sheet below 1280px). The polite region here leaves them out, so each is
 * announced once, and the permission words on the row are not hidden behind a duplicate.
 */
function announcedElsewhere(session: SessionView | null): boolean {
  const state = session?.state;
  return state === "permission" || state === "ended" || state === "failed";
}

/**
 * The state as a glyph: the roundel for every busy state (starting, working, ending), the warning
 * mark for a decision, the quiet LED for a chat waiting or none. Each is hidden from assistive
 * technology; the words beside it carry the state. Exported so the head can draw the same glyph
 * beside its state word; inside the panel the head's roundel is the one that turns, and the
 * row's copy is hidden under it (chat-surface.css).
 */
export function StateGlyph({ state }: { state: SessionView["state"] | undefined }) {
  if (state !== undefined && BUSY_STATES.has(state)) {
    return (
      // The disc is 86% of the glyph box, so this box draws it at 14px.
      <RoundelGlyph className="chat-state-glyph" size="calc(14px / 0.86)" />
    );
  }
  if (state === "permission") {
    return (
      <span aria-hidden="true" className="chat-state-glyph text-(--tone-warning)">
        ⚠
      </span>
    );
  }
  return <span aria-hidden="true" className="chat-led" />;
}

/** A failure notice: a toned glyph and a title over plain text, with no box. */
function Notice({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="chat-notice" data-tone="error">
      <p className="chat-notice-title">
        <span aria-hidden="true" className="chat-notice-glyph">
          ⚠
        </span>
        {title}
      </p>
      {children}
    </div>
  );
}

function Failure({ session, now }: { session: SessionView; now: number }) {
  const title =
    session.failure?.kind === "spawn"
      ? "The session could not start"
      : "The session stopped unexpectedly";
  return (
    <Notice title={title}>
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
    </Notice>
  );
}

/**
 * `hideWords` hides the state words from browse mode (the sr-only region announces them);
 * `detail` (elapsed time, newest tool, subagents) is not announced there, so it stays exposed.
 * The detail stays one text run (its test reads it whole); while working, chat-surface.css keeps
 * it on one line and cuts the tool's summary with an ellipsis, so the row's `title` holds all of it.
 */
function StateRow(props: {
  session: SessionView | null;
  text: string;
  detail: string;
  hideWords: boolean;
}) {
  const { session, text, detail, hideWords } = props;
  return (
    <div className="chat-state-row flex items-center gap-2">
      <StateGlyph state={session?.state} />
      {session?.state === "starting" ? (
        <StartingBrief session={session} />
      ) : (
        <span className="chat-state-text" title={detail || undefined}>
          <span className="chat-state-words" aria-hidden={hideWords || undefined}>
            <Untrusted text={text} />
          </span>
          {detail && (
            <>
              <span className="chat-state-sep"> · </span>
              <Untrusted text={detail} />
            </>
          )}
        </span>
      )}
    </div>
  );
}

function StateBody(props: {
  session: SessionView | null;
  text: string;
  detail: string;
  hideWords: boolean;
  now: number;
}) {
  const { session, text, detail, hideWords, now } = props;
  if (session?.state === "failed") return <Failure session={session} now={now} />;
  if (session?.state === "ended") {
    // Plain text, exposed to browse mode but not a live region: ChatAnnouncer says it aloud once.
    // The head's status word already says the chat ended; the row gives its age.
    return <p className="text-muted-foreground">{`${age(session.state_since, now)} ago`}</p>;
  }
  return <StateRow session={session} text={text} detail={detail} hideWords={hideWords} />;
}

export function StateLine() {
  const session = useAtomValue(panelSessionAtom);
  const turns = useAtomValue(turnsAtom);
  const pending = useAtomValue(pendingPermissionsAtom);
  const now = useNow();
  const words = stateWords(session, pending);
  const detail = session?.state === "working" ? workingDetail(session, turns, now).join(" · ") : "";
  // The sr-only region announces the words, so the visible copy is hidden from browse mode on the
  // rows that hold only those words (and buttons beside them); the Starting row carries more than
  // the announcement and stays exposed.
  const elsewhere = announcedElsewhere(session);
  const hideWords = !elsewhere && session?.state !== "starting";
  return (
    <div
      className="chat-state-line"
      data-kind={session?.kind}
      data-state={session?.state ?? "none"}
    >
      <div role="status" aria-live="polite" aria-atomic="true" className="sr-only">
        <Untrusted text={elsewhere ? "" : words} />
      </div>
      <div aria-live="off">
        <StateBody session={session} text={words} detail={detail} hideWords={hideWords} now={now} />
      </div>
    </div>
  );
}
