import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { PanelRightClose } from "lucide-react";
import { useLayoutEffect, useRef, type RefObject } from "react";

import { useMediaQuery } from "@/lib/use-media-query";
import { pendingAttachmentsAtom } from "@/plan/attachments";
import { ChatGlyph, STATE_NAME } from "./chat-state-glyph";
import { pendingPermissionsAtom } from "./model";
import { CHAT_PANEL_ID, chatCollapsedAtom, chatSheetOpenAtom, DESKTOP } from "./sheet-state";
import { shownSessionAtom } from "./shown-session";

const TRIGGER = "[data-chat-sheet-trigger]";
const COLLAPSE = "[data-chat-collapse]";

/** The count is drawn for sight; the button's accessible name carries it for everyone else. */
function PermissionBadge() {
  const count = useAtomValue(pendingPermissionsAtom).length;
  return count ? (
    <span className="chat-permission-badge" aria-hidden="true">
      {count}
    </span>
  ) : null;
}

/** An aria-label replaces a button's contents, so the state and counts are spelled into the name. */
function useChatButtonName(verb: string, attachments = 0): string {
  const session = useAtomValue(shownSessionAtom);
  const decisions = useAtomValue(pendingPermissionsAtom).length;
  const parts = [verb];
  if (decisions > 0) parts.push(`${decisions} decision${decisions === 1 ? "" : "s"} waiting`);
  else if (session) parts.push(STATE_NAME[session.state]);
  if (attachments > 0) parts.push(`${attachments} attachment${attachments === 1 ? "" : "s"}`);
  return parts.join(", ");
}

/**
 * The header's Chat button. Below 1280px it opens the sheet; from 1280px it shows only while the
 * side panel is collapsed, and expands it, so the decisions waiting stay in sight either way.
 */
export function ChatSheetButton() {
  const desktop = useMediaQuery(DESKTOP);
  const [open, setOpen] = useAtom(chatSheetOpenAtom);
  const [collapsed, setCollapsed] = useAtom(chatCollapsedAtom);
  const attachments = useAtomValue(pendingAttachmentsAtom).length;
  const name = useChatButtonName(desktop ? "Expand chat" : "Open chat", attachments);
  if (desktop && !collapsed) return null;
  return (
    <button
      type="button"
      data-chat-sheet-trigger
      aria-label={name}
      aria-haspopup={desktop ? undefined : "dialog"}
      aria-expanded={desktop ? false : open}
      aria-controls={desktop ? CHAT_PANEL_ID : undefined}
      onClick={() => (desktop ? setCollapsed(false) : setOpen(true))}
      className="inline-flex items-center gap-1.5 text-[13px]"
    >
      <ChatGlyph />
      <span>Chat</span>
      <PermissionBadge />
      {attachments > 0 && <span aria-hidden="true">{attachments}</span>}
    </button>
  );
}

/** The side panel's head control; the header's Chat button expands the panel again. */
export function CollapseChatButton() {
  const setCollapsed = useSetAtom(chatCollapsedAtom);
  return (
    <button
      type="button"
      className="chat-head-button"
      data-chat-collapse
      aria-label="Collapse chat"
      aria-expanded
      aria-controls={CHAT_PANEL_ID}
      onClick={() => setCollapsed(true)}
    >
      <PanelRightClose size={15} aria-hidden="true" />
    </button>
  );
}

/**
 * Keeps focus when the side panel changes: collapsing with focus inside the panel moves it to the
 * header's Expand chat, and expanding after that button left the page moves it to Collapse chat.
 * Runs only on a change, so the first render never takes focus.
 */
export function useCollapseFocus(panel: RefObject<HTMLElement | null>, collapsed: boolean): void {
  const was = useRef(collapsed);
  useLayoutEffect(() => {
    if (was.current === collapsed) return;
    was.current = collapsed;
    const active = document.activeElement;
    if (collapsed) {
      if (panel.current?.contains(active)) document.querySelector<HTMLElement>(TRIGGER)?.focus();
    } else if (!active || active === document.body) {
      panel.current?.querySelector<HTMLElement>(COLLAPSE)?.focus();
    }
  }, [collapsed, panel]);
}
