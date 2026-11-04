import { useAtomValue, useSetAtom } from "jotai";
import { MoreHorizontal, PanelRightClose, X } from "lucide-react";

import type { SessionView } from "@/plan/api/schema-chat";
import { STATE_NAME } from "./chat-state-glyph";
import { EndChatMenuItem, useCanEndChat } from "./end-chat";
import { panelCollapsedAtom } from "./prefs";
import { panelSessionAtom } from "./session-home";
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

/**
 * The ⋯ menu exists only while a chat is live, and holds End chat alone. Starting fresh is End chat,
 * then New chat: New chat is never drawn beside a live chat.
 */
function ChatMenu() {
  const canEnd = useCanEndChat();
  if (!canEnd) return null;
  return (
    <details className="chat-menu relative">
      <summary aria-label="Chat menu" className="chat-head-button">
        <MoreHorizontal size={15} aria-hidden="true" />
      </summary>
      <div className="chat-menu-pop">
        <EndChatMenuItem />
      </div>
    </details>
  );
}

/**
 * The panel's head: one line holding what this chat is, what it is doing and the window
 * controls, drawn with or without a session. Both bodies of panel.tsx render this head. What the
 * user is reading is not here: the composer carries it (`ContextLine`, context-line.tsx).
 */
export function PanelHead({ sheet }: { sheet: boolean }) {
  const session = useAtomValue(panelSessionAtom);
  const setCollapsed = useSetAtom(panelCollapsedAtom);
  const setOpen = useSetAtom(chatSheetOpenAtom);
  const Close = sheet ? X : PanelRightClose;
  const title = headTitle(session, useSessionBrief(session?.id ?? null));
  return (
    <header className="chat-head">
      {/* The focus fallback for dialogs whose opener has left the page (see BriefDialog). */}
      {/* The title keeps one line; `title` holds the whole text when it is cut short. */}
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
        <ChatMenu />
        <button
          type="button"
          className="chat-head-button"
          aria-label={sheet ? "Close chat" : "Collapse chat"}
          onClick={() => (sheet ? setOpen(false) : setCollapsed(true))}
        >
          <Close size={15} aria-hidden="true" />
        </button>
      </div>
    </header>
  );
}
