import { useAtomValue } from "jotai";
import { ChevronDown } from "lucide-react";
import { useEffect, useId, useRef, useState, type JSX } from "react";

import type { SessionView } from "@/plan/api/schema-chat";
import { backgroundAtom } from "@/plan/state/atoms";
import { plural } from "@/plan/ui/plural";
import { waitsOnBackground } from "./background-fold";
import { unendedBackgroundCountAtom } from "./background-tasks";

import "./background.css";

const FINISHED = "Background tasks finished";

/**
 * How long an announcement stays in the status region. The region is read out when its text
 * changes; left there, a browse-mode reader would find a count the visible line has moved past.
 * Seven seconds is long enough for every screen reader to have picked the change up.
 */
const ANNOUNCE_HOLD_MS = 7000;

function waitingWords(unended: number): string {
  return `Waiting on ${plural(unended, "background task")}`;
}

/** What the session's task table says now. */
interface Table {
  waiting: boolean;
  unended: number;
  /** A frame for this session has arrived: zero unended tasks then means they ended. */
  known: boolean;
}

/**
 * What the status region has said for one session. `told` holds from the "Waiting on" announcement
 * until the last task ends, so the line is announced once however often the chat leaves idle and
 * returns. `message` is the region's text; null draws no region at all.
 */
interface Said {
  session: string | null;
  told: boolean;
  message: string | null;
}

const SILENT: Said = { session: null, told: false, message: null };

/** The next `Said`; the same object when nothing changes, which ends the render-time update. */
function nextSaid(said: Said, table: Table): Said {
  if (table.unended === 0) {
    // After a reconnect the table is unknown until its frame arrives: that is not an ending.
    if (said.told && table.known) return { ...said, told: false, message: FINISHED };
    return !said.told && said.message === "" ? { ...said, message: null } : said;
  }
  if (said.told) return said;
  if (table.waiting) return { ...said, told: true, message: waitingWords(table.unended) };
  // Tasks run while the chat works: the empty region is in place before its first words.
  return said.message === "" ? said : { ...said, message: "" };
}

/**
 * The status region's text: "Waiting on N background tasks" once, when the line first shows, and
 * "Background tasks finished" once, when the last task ends. A changed count and a row's change of
 * state are not announced. Null until the session has had a task.
 */
function useAnnouncement(session: string | null, table: Table): string | null {
  const [said, setSaid] = useState(SILENT);
  const next = nextSaid(said.session === session ? said : { ...SILENT, session }, table);
  // State derived from the table as it changes, set while rendering so no stale frame is drawn.
  if (next !== said) setSaid(next);
  const { message } = next;
  useEffect(() => {
    if (!message) return;
    const clear = (now: Said): Said => (now.message === message ? { ...now, message: "" } : now);
    const timer = window.setTimeout(() => setSaid(clear), ANNOUNCE_HOLD_MS);
    return () => window.clearTimeout(timer);
  }, [message]);
  return message;
}

/** One head's background line and list: the head calls the hook once and hands it to both. */
export interface BackgroundDisclosure {
  /** The line is drawn: the session is idle and waits on at least one task. */
  waiting: boolean;
  /** The tasks that have not ended. */
  unended: number;
  /** The list is drawn: the disclosure is pressed and its line is there. */
  open: boolean;
  listId: string;
  /** The status region's text; null draws no region. */
  announcement: string | null;
  toggle: () => void;
  close: () => void;
}

/**
 * The state `BackgroundLine` and `BackgroundList` share. They sit apart (the line inside the head's
 * sub-line, which clips, the list after it), so the head that renders both owns the open state and
 * the list's id. `session` is the session that head presents. The list is open per session: a new
 * chat starts closed, and a chat that works for a turn comes back to idle with its list as it was.
 */
export function useBackgroundDisclosure(session: SessionView | null): BackgroundDisclosure {
  const count = useAtomValue(unendedBackgroundCountAtom);
  const frame = useAtomValue(backgroundAtom);
  const [openFor, setOpenFor] = useState<string | null>(null);
  const listId = useId();
  const id = session?.id ?? null;
  // The count is the chat session's; a head presenting no session has no tasks to show.
  const unended = id === null ? 0 : count;
  const waiting = waitsOnBackground(session, unended);
  const known = id !== null && frame?.session === id;
  const announcement = useAnnouncement(id, { waiting, unended, known });
  const open = waiting && openFor === id;
  return {
    waiting,
    unended,
    open,
    listId,
    announcement,
    toggle: () => setOpenFor(open ? null : id),
    close: () => setOpenFor(null),
  };
}

/**
 * The one thing an idle chat says: "Waiting on N background tasks", with the disclosure of the
 * task list. It renders nothing in every other case (a chat that is not idle, or no task left to
 * end), so an idle chat with no task still draws and announces nothing. It draws no glyph: the head
 * has no busy sign, and the list's rows carry the tasks' marks.
 */
export function BackgroundLine({
  disclosure,
}: {
  disclosure: BackgroundDisclosure;
}): JSX.Element | null {
  const { waiting, unended, open, listId, toggle, close } = disclosure;
  const button = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!open) return;
    const onKey = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape" || event.isComposing || event.target !== button.current) return;
      // Taken at the window and stopped there: below 1280px the head sits in the sheet's dialog,
      // which listens for Escape on the document in the capture phase, ahead of any React
      // handler, and would close with the list. Focus is on the disclosure and stays there.
      event.preventDefault();
      event.stopPropagation();
      close();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [open, close]);
  if (!waiting) return null;
  return (
    <span className="chat-background-line">
      <span className="chat-background-words">{waitingWords(unended)}</span>
      <button
        ref={button}
        type="button"
        className="chat-background-toggle"
        aria-label="Background tasks"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        onClick={toggle}
      >
        <ChevronDown size={14} aria-hidden="true" />
      </button>
    </span>
  );
}
