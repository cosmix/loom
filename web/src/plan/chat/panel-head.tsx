import { useAtomValue, useSetAtom } from "jotai";
import { X } from "lucide-react";

import type { SessionView } from "@/plan/api/schema-chat";
import { panelSessionAtom, sessionHomeAtom } from "./session-home";
import { chatSheetOpenAtom } from "./sheet-state";
import { hasStateWords, StateLine } from "./state-line";
import { NewChatButton } from "./start-chat";
import { useSessionBrief, type BriefState } from "./use-session-brief";
import { contextDocumentAtom } from "./viewed-document";
import { Untrusted, visibleControls } from "./visible-controls";

/**
 * What the head calls a session. Only an authoring chat this server started has a brief; a pressure
 * test never does, and a chat recovered after a restart has lost its. `session.name` is the process
 * label (`loom plan: chat`, `recovered authoring session`), so it is a title only when it starts
 * like one.
 */
function headTitle(session: SessionView | null, brief: BriefState): string {
  if (!session) return "New chat";
  if (brief.status === "ready") return brief.brief.label;
  if (session.kind === "pressure") return "Pressure test";
  if (brief.status === "loading") return "Chat";
  return /^\p{Lu}/u.test(session.name) ? session.name : "Chat";
}

/** The place the chat concerns: its session's feature or plan, else the document being read. */
function usePlace(session: SessionView | null): string {
  const home = useAtomValue(sessionHomeAtom);
  const viewed = useAtomValue(contextDocumentAtom);
  if (session) return home?.title ?? "";
  return viewed.kind !== null ? visibleControls(viewed.name) : "";
}

/**
 * The panel's head, drawn with or without a session: the title over a sub-line (the state in a few
 * words, then the place the chat concerns), and New chat and Close chat at the side. It has no
 * menu: ending a chat is New chat. Both bodies of panel.tsx render this head.
 */
export function PanelHead({ sheet }: { sheet: boolean }) {
  const session = useAtomValue(panelSessionAtom);
  const setOpen = useSetAtom(chatSheetOpenAtom);
  const title = headTitle(session, useSessionBrief(session?.id ?? null));
  const place = usePlace(session);
  return (
    <header className="chat-head">
      <div className="chat-head-text">
        {/* The focus fallback for dialogs whose opener has left the page (see BriefDialog). */}
        {/* The title keeps one line; `title` holds the whole text when it is cut short. */}
        <h2
          className="chat-head-title"
          tabIndex={-1}
          data-chat-heading
          data-new={session ? undefined : ""}
          title={visibleControls(title)}
        >
          <Untrusted text={title} />
        </h2>
        <div className="chat-head-sub">
          <StateLine />
          {place && (
            <span className="chat-head-place">
              {hasStateWords(session) && " · "}
              <Untrusted text={place} />
            </span>
          )}
        </div>
      </div>
      <div className="chat-head-side">
        <NewChatButton variant="head" />
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
