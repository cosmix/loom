import { useAtomValue, useSetAtom } from "jotai";
import { MoreHorizontal, PanelRightClose, X } from "lucide-react";

import type { SessionView } from "@/plan/api/schema-chat";
import { canAuthorAtom, serverInfoAtom } from "@/plan/state/atoms";
import { STATE_NAME } from "./chat-state-glyph";
import { ContextLine } from "./context-line";
import { EndChatMenuItem, useCanEndChat } from "./end-chat";
import { panelCollapsedAtom } from "./prefs";
import { isActiveSession, panelSessionAtom } from "./session-home";
import { chatSheetOpenAtom } from "./sheet-state";
import { NewChatButton } from "./start-chat";
import { useSessionBrief, type BriefState } from "./use-session-brief";
import { Untrusted, visibleControls } from "./visible-controls";

/**
 * What the head calls a session. Only an authoring chat this server started has a brief; a pressure
 * test never does, and a chat recovered after a restart has lost its. `session.name` is the process
 * label (`loom plan: chat`, `recovered authoring session`), so it is a title only when it starts
 * like one.
 */
function headTitle(session: SessionView | null, brief: BriefState): string {
  if (!session) return "Chat";
  if (brief.status === "ready") return brief.brief.label;
  if (session.kind === "pressure") return "Pressure test";
  if (brief.status === "loading") return "Chat";
  return /^\p{Lu}/u.test(session.name) ? session.name : "Chat";
}

/** The state as a glyph and a word; the LED takes the state's tone, the word stays muted. */
function StateWord({ state }: { state: SessionView["state"] }) {
  return (
    <span className="chat-state plan-status" data-state={state}>
      <span className="chat-led" aria-hidden="true" />
      {STATE_NAME[state]}
    </span>
  );
}

/** The ⋯ menu exists only when it holds an item. Beside a running chat it carries New chat. */
function ChatMenu({ session }: { session: SessionView | null }) {
  const canEnd = useCanEndChat();
  const canAuthor = useAtomValue(canAuthorAtom);
  const readOnly = (useAtomValue(serverInfoAtom)?.read_only.length ?? 0) > 0;
  const canStart = isActiveSession(session) && canAuthor && !readOnly;
  if (!canEnd && !canStart) return null;
  return (
    <details className="chat-menu relative">
      <summary aria-label="Chat menu" className="chat-head-button">
        <MoreHorizontal size={15} aria-hidden="true" />
      </summary>
      <div className="chat-menu-pop">
        {canEnd && <EndChatMenuItem />}
        {canStart && <NewChatButton />}
      </div>
    </details>
  );
}

/**
 * The panel's head: a title row (what this chat is, what it is doing, the window controls) and,
 * under it, the context line (`<ContextLine />`, context-line.tsx: "About section_05"), drawn
 * with or without a session. Both bodies of panel.tsx render this head.
 */
export function PanelHead({ sheet }: { sheet: boolean }) {
  const session = useAtomValue(panelSessionAtom);
  const setCollapsed = useSetAtom(panelCollapsedAtom);
  const setOpen = useSetAtom(chatSheetOpenAtom);
  const Close = sheet ? X : PanelRightClose;
  const title = headTitle(session, useSessionBrief(session?.id ?? null));
  return (
    <header className="chat-head">
      <div className="chat-head-row">
        {/* The focus fallback for dialogs whose opener has left the page (see BriefDialog). */}
        {/* The title wraps to two lines; `title` keeps the whole text when it is clamped. */}
        <h2
          className="chat-head-title"
          tabIndex={-1}
          data-chat-heading
          title={visibleControls(title)}
        >
          <Untrusted text={title} />
        </h2>
        <div className="chat-head-side">
          {session ? <StateWord state={session.state} /> : <NewChatButton />}
          <ChatMenu session={session} />
          <button
            type="button"
            className="chat-head-button"
            aria-label={sheet ? "Close chat" : "Collapse chat"}
            onClick={() => (sheet ? setOpen(false) : setCollapsed(true))}
          >
            <Close size={15} aria-hidden="true" />
          </button>
        </div>
      </div>
      <ContextLine />
    </header>
  );
}
