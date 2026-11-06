import { useAtomValue } from "jotai";
import { memo, useId, useLayoutEffect, useRef, useState, type JSX } from "react";

import type { EndedChat, Item, SessionView, Turn } from "@/plan/api/schema-chat";
import { plural } from "@/plan/ui/plural";
import { closingTicks, isClosingTurn, leftBehind, userWrote } from "./closing";
import { FailureNotice } from "./failure-notice";
import { ItemView } from "./items";
import { pendingPermissionsAtom, sessionAtom, snapshotAtom, turnsAtom } from "./model";
import { PermissionCard } from "./permission-card";
import { SessionDivider } from "./session-divider";
import { clearedChatAtom, shownSessionAtom } from "./shown-session";
import { groupToolRows, type ToolGroup } from "./tool-group";
import { TurnFooter } from "./turn-footer";
import { Untrusted } from "./visible-controls";
import { LeftBehindCard, StartingCard, WrapUpCard } from "./wrap-up";
import "./chat-surface.css";

type ToolItem = Extract<Item, { type: "tool" }>;
type TurnProps = {
  turn: Turn;
  sessionId: string;
  number: number;
  collapsed: boolean;
  /** The turn the model is working on now: it ends with the typing bubble. */
  working: boolean;
};

function chronological(turns: Turn[]): Turn[] {
  return [...turns].sort((a, b) => Date.parse(a.started_at) - Date.parse(b.started_at));
}

function nestedTools(turn: Turn): ToolItem[] {
  const parents = new Set(
    turn.items
      .filter((item): item is ToolItem => item.type === "tool" && item.name === "Agent")
      .map((item) => item.tool_use_id),
  );
  return turn.items.filter(
    (item): item is ToolItem =>
      item.type === "tool" &&
      item.subagent !== null &&
      item.parent !== null &&
      parents.has(item.parent),
  );
}

function TranscriptItem({
  item,
  turn,
  nested,
}: {
  item: Item;
  turn: Turn;
  nested: ToolItem[];
}): JSX.Element {
  return (
    <div
      aria-live={item.type === "text" ? undefined : "off"}
      aria-atomic={item.type === "text" ? true : undefined}
      className={item.type === "tool" ? "chat-tool-run" : undefined}
    >
      <ItemView
        item={item}
        turn={turn}
        nested={
          item.type === "tool"
            ? nested.filter((child) => child.parent === item.tool_use_id)
            : undefined
        }
      />
    </div>
  );
}

function FoldedTools({
  group,
  turn,
  nested,
}: {
  group: ToolGroup;
  turn: Turn;
  nested: ToolItem[];
}): JSX.Element {
  // The working card: open while the turn runs, folded to its line once the turn ends. The
  // reader's own toggle sticks until the turn's state changes, which is when `open` re-renders.
  return (
    <details aria-live="off" className="chat-tool-fold" open={turn.open}>
      <summary>
        <Untrusted text={group.label} />
      </summary>
      {group.items.map((item) => (
        <TranscriptItem key={item.id} item={item} turn={turn} nested={nested} />
      ))}
    </details>
  );
}

function TurnItems({ turn }: { turn: Turn }): JSX.Element {
  const nested = nestedTools(turn);
  const nestedIds = new Set(nested.map((item) => item.id));
  // A pending request is drawn once, at the end of the transcript (PendingDecision). Thinking
  // markers draw nothing, so they are dropped before tool rows are grouped.
  const items = turn.items.filter(
    (item) =>
      !nestedIds.has(item.id) &&
      item.type !== "thinking" &&
      !(item.type === "permission" && item.status === "pending"),
  );
  return (
    <div className="chat-turn-items">
      {groupToolRows(items).map((row) =>
        "items" in row ? (
          <FoldedTools key={`tools-${row.items[0]?.id}`} group={row} turn={turn} nested={nested} />
        ) : (
          <TranscriptItem key={row.id} item={row} turn={turn} nested={nested} />
        ),
      )}
    </div>
  );
}

function turnLabel(turn: Turn, number: number): string {
  return `Turn ${number} · ${plural(turn.changed_files.length, "file")} changed`;
}

/** The assistant's typing bubble while the model works; the words are for screen readers. */
function TypingBubble(): JSX.Element {
  return (
    <div className="chat-typing" aria-live="off">
      <span className="chat-typing-dot" aria-hidden="true" />
      <span className="chat-typing-dot" aria-hidden="true" />
      <span className="chat-typing-dot" aria-hidden="true" />
      <span className="sr-only">Working</span>
    </div>
  );
}

/** Memoised on the turn object: a delta replaces only the turn it touches, so the rest skip. */
const TranscriptTurn = memo(function TranscriptTurn({
  turn,
  sessionId,
  number,
  collapsed,
  working,
}: TurnProps): JSX.Element {
  const [expanded, setExpanded] = useState(false);
  const contentId = useId();
  const shown = !collapsed || expanded;
  return (
    <section className="chat-turn" aria-label={`Turn ${number}`}>
      {collapsed && (
        <button
          type="button"
          aria-live="off"
          aria-expanded={expanded}
          aria-controls={contentId}
          className="chat-turn-toggle"
          onClick={() => setExpanded((value) => !value)}
        >
          {turnLabel(turn, number)}
        </button>
      )}
      <div
        id={contentId}
        hidden={!shown}
        aria-live={collapsed ? "off" : undefined}
        className={shown ? "chat-turn" : undefined}
      >
        {shown && (
          <>
            <TurnItems turn={turn} />
            <div aria-live="off">
              <TurnFooter turn={turn} sessionId={sessionId} />
            </div>
            {working && <TypingBubble />}
          </>
        )}
      </div>
    </section>
  );
});

/** The first pending request, with its "1 of N"; ChatAnnouncer is the one that says it aloud. */
function PendingDecision(): JSX.Element | null {
  const first = useAtomValue(pendingPermissionsAtom)[0];
  if (!first) return null;
  return (
    <div aria-live="off">
      <PermissionCard key={first.request_id} item={first} />
    </div>
  );
}

function useFollow(turns: Turn[], previous: unknown) {
  const scroller = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const following = useRef(true);
  const [activity, setActivity] = useState(false);
  const follow = () => {
    const node = scroller.current;
    if (node) node.scrollTop = node.scrollHeight;
    following.current = true;
    setActivity(false);
  };
  const onScroll = () => {
    const node = scroller.current;
    if (!node) return;
    following.current = node.scrollHeight - node.clientHeight - node.scrollTop <= 32;
    if (following.current) setActivity(false);
  };
  useLayoutEffect(() => {
    if (following.current) {
      const node = scroller.current;
      if (node) node.scrollTop = node.scrollHeight;
    } else setActivity(true);
  }, [turns, previous]);
  useLayoutEffect(() => {
    if (typeof ResizeObserver === "undefined" || !content.current) return;
    const observer = new ResizeObserver(() => {
      const node = scroller.current;
      if (following.current && node) node.scrollTop = node.scrollHeight;
    });
    observer.observe(content.current);
    return () => observer.disconnect();
  }, []);
  return { scroller, content, activity, follow, onScroll };
}

/**
 * The closing turn is never a turn: a card while its write runs, then what the chat left behind.
 * A closing turn still open in a chat that is not `ending` (the write was cut off) is left behind.
 */
function ClosingCard({
  turn,
  chat,
  sessionId,
}: {
  turn: Turn;
  chat: Turn[];
  sessionId: string;
}): JSX.Element {
  const shown = useAtomValue(shownSessionAtom);
  const writing = turn.open && shown?.id === sessionId && shown.state === "ending";
  return writing ? (
    <WrapUpCard ticks={closingTicks(turn)} />
  ) : (
    <LeftBehindCard summary={leftBehind(chat)} />
  );
}

type ChatTurnsProps = {
  turns: Turn[];
  sessionId: string;
  /** Turns of this conversation drawn before `turns`, and how many of all of them fold. */
  before: number;
  cutoff: number;
  workingId: string | null;
};

function ChatTurns({ turns, sessionId, before, cutoff, workingId }: ChatTurnsProps): JSX.Element {
  return (
    <>
      {turns.map((turn, index) =>
        isClosingTurn(turn) ? (
          <ClosingCard
            key={`${sessionId}/${turn.id}`}
            turn={turn}
            chat={turns}
            sessionId={sessionId}
          />
        ) : (
          <TranscriptTurn
            key={`${sessionId}/${turn.id}`}
            turn={turn}
            sessionId={sessionId}
            number={index + 1}
            collapsed={before + index < cutoff}
            working={turn.id === workingId}
          />
        ),
      )}
    </>
  );
}

function newestOpenId(turns: Turn[], session: SessionView | null): string | null {
  // A pressure run's running steps carry their own turning arrows; its transcript draws no typing
  // bubble.
  if (session?.state !== "working" || session.kind === "pressure") return null;
  return turns.findLast((turn) => turn.open && !isClosingTurn(turn))?.id ?? null;
}

/** What New chat cleared is not drawn: the concluded chat's id is `clearedChatAtom`. */
function TurnList({
  turns,
  session,
  previous,
}: {
  turns: Turn[];
  session: SessionView | null;
  previous: EndedChat | null;
}): JSX.Element {
  const cleared = useAtomValue(clearedChatAtom);
  const concluded = session?.state === "ended" || session?.state === "failed";
  const current = chronological(concluded && session?.id === cleared ? [] : turns);
  const shownPrevious = previous && previous.session.id !== cleared ? previous : null;
  const earlier = chronological(shownPrevious?.turns ?? []);
  const cutoff = Math.max(0, earlier.length + current.length - 20);
  return (
    <>
      {shownPrevious && (
        <div aria-live="off" className="chat-transcript-flow">
          <ChatTurns
            turns={earlier}
            sessionId={shownPrevious.session.id}
            before={0}
            cutoff={cutoff}
            workingId={null}
          />
          <SessionDivider previous={shownPrevious} next={session} />
        </div>
      )}
      <ChatTurns
        turns={current}
        sessionId={session?.id ?? ""}
        before={earlier.length}
        cutoff={cutoff}
        workingId={newestOpenId(current, session)}
      />
    </>
  );
}

/**
 * The end of the flow, by the shown session's state: the starting brief, the wrap-up before the
 * closing turn exists, a failure, and what an ended chat left behind when its closing write never
 * ran or was cut off (a closing turn draws its own card).
 */
function ChatOutcome({ turns }: { turns: Turn[] }): JSX.Element | null {
  const shown = useAtomValue(shownSessionAtom);
  if (!shown) return null;
  const closed = turns.some(isClosingTurn);
  switch (shown.state) {
    case "starting":
      return <StartingCard session={shown} />;
    case "failed":
      return <FailureNotice session={shown} />;
    case "ending":
      return userWrote(turns) && !closed ? <WrapUpCard ticks={[]} /> : null;
    case "ended": {
      if (closed || !userWrote(turns)) return null;
      const summary = leftBehind(turns);
      return summary.documents.length > 0 ? <LeftBehindCard summary={summary} /> : null;
    }
    default:
      return null;
  }
}

export function Transcript(): JSX.Element {
  const turns = useAtomValue(turnsAtom);
  const session = useAtomValue(sessionAtom);
  const { previous } = useAtomValue(snapshotAtom);
  const scroll = useFollow(turns, previous);
  // The conversation the outcome belongs to: the live session's turns, else the concluded one's.
  const chat = chronological(session ? turns : (previous?.turns ?? []));
  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <div
        ref={scroll.scroller}
        onScroll={scroll.onScroll}
        role="log"
        aria-label="Chat transcript"
        aria-live="polite"
        aria-relevant="additions"
        aria-atomic="false"
        tabIndex={0}
        className="chat-transcript min-h-0 flex-1 overflow-y-auto overscroll-contain font-sans leading-[1.5]"
        style={{ unicodeBidi: "plaintext", direction: "ltr", overflowAnchor: "none" }}
      >
        <div ref={scroll.content} className="chat-transcript-flow">
          <TurnList turns={turns} session={session} previous={previous} />
          <PendingDecision />
          <ChatOutcome turns={chat} />
        </div>
      </div>
      {scroll.activity && (
        <button type="button" onClick={scroll.follow} className="chat-follow">
          New activity
        </button>
      )}
    </div>
  );
}
