import { useAtom, useAtomValue, useSetAtom, useStore } from "jotai";
import { Dialog } from "radix-ui";
import { useEffect, useRef, type JSX, type KeyboardEvent, type PointerEvent } from "react";

import { useMediaQuery } from "@/lib/use-media-query";
import { pendingAttachmentsAtom } from "@/plan/attachments";
import { useReviewLive } from "@/plan/review/live";
import { registerSendToAuthor } from "@/plan/review/send-to-author";
import { AnnouncerRegions, ChatAnnouncer } from "./announcer";
import { useAttention } from "./attention";
import { BriefDialogHost } from "./brief-dialog";
import { ChatGlyph, STATE_NAME } from "./chat-state-glyph";
import { ComposerArea } from "./composer";
import { ConnectionBanner, useConnectionDim } from "./connection-banner";
import { EndChatMenuItem, useCanEndChat } from "./end-chat";
import { pendingPermissionsAtom, sessionAtom, useChatSocket } from "./model";
import { offerErrorAtom } from "./offer-error";
import { createOfferStarter } from "./offer-flow";
import { registerAuthoringOffers } from "./offer-handlers";
import { panelCollapsedAtom, panelWidthAtom } from "./prefs";
import { RunHeader } from "./run-header";
import { chatSheetOpenAtom } from "./sheet-state";
import { shownSessionAtom } from "./shown-session";
import { NewChatButton } from "./start-chat";
import { StateLine } from "./state-line";
import { useQueueRefill } from "./stop-control";
import { useComposerShortcut } from "./composer-hooks";
import { useSuggestionFeed } from "./suggestion-cards";
import { Transcript } from "./transcript";
import { Untrusted } from "./visible-controls";
import "./chat.css";

export { chatSheetOpenAtom } from "./sheet-state";

const DESKTOP = "(min-width: 1024px)";

/** The chat socket and attention watcher for routes that mount no ChatPanel, which owns them there. */
function ChatConnection(): null {
  const store = useStore();
  useChatSocket(store);
  useAttention();
  return null;
}

/**
 * Mounts once beside the routed page so offers, Send to author, live review, the announcements
 * and the Brief dialog work at every width, with the panel collapsed or the sheet closed.
 * `connect` also opens the chat socket and attention watcher, for routes without a ChatPanel.
 */
export function ChatRegistrations({ connect = false }: { connect?: boolean }): JSX.Element {
  const store = useStore();
  const desktop = useMediaQuery(DESKTOP);
  const sheetOpen = useAtomValue(chatSheetOpenAtom);
  useReviewLive();
  useSuggestionFeed();
  useQueueRefill();
  useComposerShortcut();
  useEffect(() => {
    const starter = createOfferStarter(store);
    const unregisterOffers = registerAuthoringOffers(({ offer, feature }) =>
      starter.start(offer, feature),
    );
    const unregisterSelection = registerSendToAuthor();
    return () => {
      unregisterSelection();
      unregisterOffers();
      starter.dispose();
    };
  }, [store]);
  return (
    <>
      <ChatAnnouncer inSheet={!desktop && sheetOpen} />
      <BriefDialogHost />
      {connect && <ChatConnection />}
    </>
  );
}

/** A refused offer start, shown inside the panel's landmark until dismissed. */
function OfferError() {
  const [error, setError] = useAtom(offerErrorAtom);
  if (!error) return null;
  return (
    <p role="alert" className="flex items-start gap-2 px-3 text-[12px]">
      <span className="mr-auto">
        <Untrusted text={error} />
      </span>
      <button type="button" onClick={() => setError(null)}>
        Dismiss
      </button>
    </p>
  );
}

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

export function ChatSheetButton() {
  const desktop = useMediaQuery(DESKTOP);
  const [open, setOpen] = useAtom(chatSheetOpenAtom);
  const attachments = useAtomValue(pendingAttachmentsAtom).length;
  const name = useChatButtonName("Open chat", attachments);
  if (desktop) return null;
  return (
    <button
      type="button"
      data-chat-sheet-trigger
      aria-label={name}
      aria-haspopup="dialog"
      aria-expanded={open}
      onClick={() => setOpen(true)}
      className="inline-flex items-center gap-1.5 text-[13px]"
    >
      <ChatGlyph />
      <span>Chat</span>
      <PermissionBadge />
      {attachments > 0 && <span aria-hidden="true">{attachments}</span>}
    </button>
  );
}

function CollapsedRail() {
  const setCollapsed = useSetAtom(panelCollapsedAtom);
  const name = useChatButtonName("Expand chat");
  return (
    <aside aria-label="Chat" className="chat-rail" style={{ width: "2.5rem", flexShrink: 0 }}>
      <button
        type="button"
        aria-label={name}
        onClick={() => setCollapsed(false)}
        className="flex w-full flex-col items-center gap-2 py-3"
      >
        <ChatGlyph />
        <PermissionBadge />
      </button>
    </aside>
  );
}

type Drag = { x: number; width: number; rem: number; pointer: number };

function useResizeDrag(width: number, setWidth: (width: number) => void) {
  const drag = useRef<Drag | null>(null);
  const move = (event: PointerEvent<HTMLDivElement>) => {
    const start = drag.current;
    if (start && start.pointer === event.pointerId)
      setWidth(start.width + (start.x - event.clientX) / start.rem);
  };
  return {
    onPointerDown(event: PointerEvent<HTMLDivElement>) {
      if (event.button !== 0) return;
      event.preventDefault();
      event.currentTarget.setPointerCapture(event.pointerId);
      drag.current = {
        x: event.clientX,
        width,
        pointer: event.pointerId,
        rem: Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16,
      };
    },
    onPointerMove: move,
    onPointerUp(event: PointerEvent<HTMLDivElement>) {
      if (drag.current?.pointer !== event.pointerId) return;
      move(event);
      drag.current = null;
      if (event.currentTarget.hasPointerCapture(event.pointerId))
        event.currentTarget.releasePointerCapture(event.pointerId);
    },
    onPointerCancel() {
      drag.current = null;
    },
    onLostPointerCapture() {
      drag.current = null;
    },
    onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
      event.preventDefault();
      setWidth(width + (event.key === "ArrowLeft" ? 1 : -1));
    },
  };
}

function ResizeHandle() {
  const [width, setWidth] = useAtom(panelWidthAtom);
  const handlers = useResizeDrag(width, setWidth);
  // A drag stores fractional rem; the announced value is a whole number.
  const rem = Math.round(width);
  return (
    <div
      role="separator"
      aria-label="Resize chat"
      aria-orientation="vertical"
      tabIndex={0}
      aria-valuemin={20}
      aria-valuemax={40}
      aria-valuenow={rem}
      aria-valuetext={`${rem} rem`}
      className="chat-resize-handle"
      style={{ touchAction: "none" }}
      {...handlers}
    />
  );
}

/** The ⋯ menu exists only when it holds an item. */
function ChatMenu() {
  const canEnd = useCanEndChat();
  if (!canEnd) return null;
  return (
    <details className="relative">
      <summary aria-label="Chat menu">⋯</summary>
      <div className="absolute right-0 z-10 min-w-32 bg-card p-2 shadow-(--dialog-shadow)">
        <EndChatMenuItem />
      </div>
    </details>
  );
}

function PanelBody({ sheet }: { sheet: boolean }) {
  const session = useAtomValue(sessionAtom);
  const shown = useAtomValue(shownSessionAtom);
  // A failed or ended chat carries New chat in the composer, so the header would repeat it.
  const live = shown !== null && shown.state !== "failed" && shown.state !== "ended";
  const setCollapsed = useSetAtom(panelCollapsedAtom);
  const setOpen = useSetAtom(chatSheetOpenAtom);
  const dim = useConnectionDim();
  return (
    <>
      <header className="flex items-center gap-2 p-3 text-[13px]">
        {/* The focus fallback for dialogs whose opener has left the page (see BriefDialog). */}
        <h2 className="mr-auto font-medium" tabIndex={-1} data-chat-heading>
          Chat
        </h2>
        {live && <NewChatButton />}
        <ChatMenu />
        <button
          type="button"
          aria-label={sheet ? "Close chat" : "Collapse chat"}
          onClick={() => (sheet ? setOpen(false) : setCollapsed(true))}
        >
          {sheet ? "Close" : "Collapse"}
        </button>
      </header>
      {/* StateRow's first two children are the glyph and label; offers follow as siblings. */}
      <div
        className={
          shown
            ? undefined
            : "[&_.chat-state-line_[aria-live=off]>.flex]:grid [&_.chat-state-line_[aria-live=off]>.flex]:grid-cols-[auto_1fr] [&_.chat-state-line_[aria-live=off]>.flex]:gap-y-3 [&_.chat-state-line_[aria-live=off]>.flex>:nth-child(n+3)]:col-span-2"
        }
      >
        <StateLine />
      </div>
      <OfferError />
      {session && <RunHeader session={session} />}
      <ConnectionBanner />
      <div className="flex min-h-0 flex-1 flex-col">
        {dim && <p className="px-3 text-[12px] text-muted-foreground">{dim}</p>}
        <Transcript />
      </div>
      <ComposerArea />
    </>
  );
}

function ChatSheet() {
  const [open, setOpen] = useAtom(chatSheetOpenAtom);
  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-black/20" />
        <Dialog.Content
          className="chat-sheet fixed inset-y-0 right-0 z-50 max-w-2xl outline-none"
          aria-describedby={undefined}
          onCloseAutoFocus={(event) => {
            const trigger = document.querySelector<HTMLButtonElement>("[data-chat-sheet-trigger]");
            if (trigger) {
              event.preventDefault();
              trigger.focus();
            }
          }}
        >
          <aside aria-label="Chat" className="chat-panel">
            <Dialog.Title className="sr-only">Chat</Dialog.Title>
            <PanelBody sheet />
          </aside>
          {/* The modal hides the page's regions from assistive tech; see ChatAnnouncer. */}
          <AnnouncerRegions />
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export function ChatPanel() {
  const store = useStore();
  useChatSocket(store);
  useAttention();
  const desktop = useMediaQuery(DESKTOP);
  const collapsed = useAtomValue(panelCollapsedAtom);
  const width = useAtomValue(panelWidthAtom);
  if (!desktop) return <ChatSheet />;
  if (collapsed) return <CollapsedRail />;
  return (
    <aside aria-label="Chat" className="chat-panel" style={{ width: `${width}rem`, flexShrink: 0 }}>
      <ResizeHandle />
      <PanelBody sheet={false} />
    </aside>
  );
}
