import { useAtomValue } from "jotai";
import { memo, useId, useLayoutEffect, useRef, useState, type JSX } from "react";

import { compactTokens, fresh } from "@/lib/tokens";
import type { EndedChat, Item, SessionView, Turn } from "@/plan/api/schema-chat";
import { ItemView } from "./items";
import { pendingPermissionsAtom, sessionAtom, snapshotAtom, turnsAtom } from "./model";
import { PermissionCard } from "./permission-card";
import { SessionDivider } from "./session-divider";
import { groupToolRows, type ToolGroup } from "./tool-group";
import { TurnFooter } from "./turn-footer";
import { Untrusted } from "./visible-controls";
import "./chat-surface.css";

type ToolItem = Extract<Item, { type: "tool" }>;
type TurnProps = { turn: Turn; sessionId: string; number: number; collapsed: boolean };

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
  return (
    <details aria-live="off" className="min-w-0 text-[13px]">
      <summary className="cursor-pointer rounded px-2 py-1.5 text-muted-foreground focus-visible:outline-2 focus-visible:outline-ring">
        <Untrusted text={group.label} />
      </summary>
      {group.items.map((item) => (
        <TranscriptItem key={item.id} item={item} turn={turn} nested={nested} />
      ))}
    </details>
  );
}

/**
 * Consecutive thinking markers become one marker whose duration is their sum. A marker without a
 * duration makes the sum unknown ("Thought"): counting it as 0s would understate the time.
 */
function mergeThinking(items: Item[]): Item[] {
  const merged: Item[] = [];
  for (const item of items) {
    const last = merged[merged.length - 1];
    if (item.type === "thinking" && last?.type === "thinking") {
      const total =
        last.duration_ms !== null && item.duration_ms !== null
          ? last.duration_ms + item.duration_ms
          : null;
      merged[merged.length - 1] = { ...last, duration_ms: total };
    } else merged.push(item);
  }
  return merged;
}

function TurnItems({ turn }: { turn: Turn }): JSX.Element {
  const nested = nestedTools(turn);
  const nestedIds = new Set(nested.map((item) => item.id));
  // A pending request is drawn once, at the end of the transcript (PendingDecision).
  const shown = turn.items.filter(
    (item) => !nestedIds.has(item.id) && !(item.type === "permission" && item.status === "pending"),
  );
  const items = mergeThinking(shown);
  return (
    <div className="chat-turn-items">
      {groupToolRows(items, !turn.open).map((row) =>
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
  const tokens = compactTokens(fresh(turn.tokens));
  return `Turn ${number} · ${turn.changed_files.length} files changed · ${tokens} tokens`;
}

/** Memoised on the turn object: a delta replaces only the turn it touches, so the rest skip. */
const TranscriptTurn = memo(function TranscriptTurn({
  turn,
  sessionId,
  number,
  collapsed,
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
          className="w-full truncate rounded py-2 text-left text-xs text-muted-foreground focus-visible:outline-2 focus-visible:outline-ring"
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

function TurnList({
  turns,
  session,
  previous,
}: {
  turns: Turn[];
  session: SessionView | null;
  previous: EndedChat | null;
}): JSX.Element {
  const current = chronological(turns);
  const earlier = chronological(previous?.turns ?? []);
  const cutoff = Math.max(0, earlier.length + current.length - 20);
  return (
    <>
      {previous && (
        <div aria-live="off" className="chat-transcript-flow">
          {earlier.map((turn, index) => (
            <TranscriptTurn
              key={`${previous.session.id}/${turn.id}`}
              turn={turn}
              sessionId={previous.session.id}
              number={index + 1}
              collapsed={index < cutoff}
            />
          ))}
          <SessionDivider previous={previous} next={session} />
        </div>
      )}
      {current.map((turn, index) => (
        <TranscriptTurn
          key={`${session?.id}/${turn.id}`}
          turn={turn}
          sessionId={session?.id ?? ""}
          number={index + 1}
          collapsed={earlier.length + index < cutoff}
        />
      ))}
    </>
  );
}

export function Transcript(): JSX.Element {
  const turns = useAtomValue(turnsAtom);
  const session = useAtomValue(sessionAtom);
  const { previous } = useAtomValue(snapshotAtom);
  const scroll = useFollow(turns, previous);
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
        className="chat-transcript min-h-0 flex-1 overflow-y-auto overscroll-contain font-sans text-[13px] leading-[1.5]"
        style={{ unicodeBidi: "plaintext", direction: "ltr", overflowAnchor: "none" }}
      >
        <div ref={scroll.content} className="chat-transcript-flow">
          <TurnList turns={turns} session={session} previous={previous} />
          <PendingDecision />
        </div>
      </div>
      {scroll.activity && (
        <button
          type="button"
          onClick={scroll.follow}
          className="absolute bottom-3 left-1/2 -translate-x-1/2 rounded-full border border-border bg-background px-3 py-1.5 text-xs font-medium shadow-sm focus-visible:outline-2 focus-visible:outline-ring"
        >
          New activity
        </button>
      )}
    </div>
  );
}
