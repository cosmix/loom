import { useAtomValue, useSetAtom } from "jotai";
import { MoreHorizontal, X } from "lucide-react";
import { useEffect, useRef, type RefObject } from "react";

import type { SessionView } from "@/plan/api/schema-chat";
import { STATE_NAME } from "./chat-state-glyph";
import { EndChatMenuItem, useCanEndChat } from "./end-chat";
import { panelSessionAtom } from "./session-home";
import { chatSheetOpenAtom } from "./sheet-state";
import { StateGlyph } from "./state-line";
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

/** The state as a glyph and a word; busy states draw the roundel, the others the LED or a mark. */
function StateWord({ state }: { state: SessionView["state"] }) {
  return (
    <span className="chat-state plan-status" data-state={state}>
      <StateGlyph state={state} />
      {STATE_NAME[state]}
    </span>
  );
}

/**
 * Closes an open `<details>` menu on Escape (focus returns to its summary) and on a pointer-down
 * outside it. Escape is taken in the capture phase on the window, ahead of a dialog's own Escape,
 * so closing the menu inside the chat sheet leaves the sheet open. Only an Escape pressed inside
 * the menu is taken: with focus elsewhere it belongs to edit mode, the help search or a popover.
 */
function useDetailsDismiss(menu: RefObject<HTMLDetailsElement | null>, active: boolean): void {
  useEffect(() => {
    const details = menu.current;
    if (!active || !details) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || !details.open) return;
      if (!(event.target instanceof Node) || !details.contains(event.target)) return;
      event.stopPropagation();
      details.open = false;
      details.querySelector("summary")?.focus();
    };
    const onPointerDown = (event: PointerEvent) => {
      if (details.open && event.target instanceof Node && !details.contains(event.target))
        details.open = false;
    };
    window.addEventListener("keydown", onKeyDown, true);
    document.addEventListener("pointerdown", onPointerDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      document.removeEventListener("pointerdown", onPointerDown);
    };
  }, [menu, active]);
}

/**
 * The ⋯ menu exists only while a chat is live, and holds End chat alone. Starting fresh is End chat,
 * then New chat: New chat is never drawn beside a live chat. The menu closes once the chat has
 * ended; a refused or failed End chat leaves it open, since its reason or error is drawn inside.
 * Focus leaving the menu closes it too.
 */
function ChatMenu() {
  const canEnd = useCanEndChat();
  const menu = useRef<HTMLDetailsElement>(null);
  useDetailsDismiss(menu, canEnd);
  if (!canEnd) return null;
  return (
    <details
      ref={menu}
      className="chat-menu relative"
      onBlur={(event) => {
        const next = event.relatedTarget;
        if (next instanceof Node && !event.currentTarget.contains(next))
          event.currentTarget.open = false;
      }}
    >
      <summary aria-label="Chat menu" className="chat-head-button">
        <MoreHorizontal size={15} aria-hidden="true" />
      </summary>
      <div className="chat-menu-pop">
        <EndChatMenuItem
          onEnded={() => {
            if (menu.current) menu.current.open = false;
          }}
        />
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
  const setOpen = useSetAtom(chatSheetOpenAtom);
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
        {sheet && (
          <button
            type="button"
            className="chat-head-button"
            aria-label="Close chat"
            onClick={() => setOpen(false)}
          >
            <X size={15} aria-hidden="true" />
          </button>
        )}
      </div>
    </header>
  );
}
