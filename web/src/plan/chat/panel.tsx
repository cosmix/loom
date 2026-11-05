import { useAtom, useAtomValue, useStore } from "jotai";
import { Dialog } from "radix-ui";
import { useEffect, useRef, type JSX, type KeyboardEvent, type PointerEvent } from "react";
import { Link } from "react-router";

import { useMediaQuery } from "@/lib/use-media-query";
import { pendingAttachmentsAtom } from "@/plan/attachments";
import { useReviewLive } from "@/plan/review/live";
import { registerSendToAuthor } from "@/plan/review/send-to-author";
import { readingFaceAtom, readingSizeAtom } from "@/plan/settings/preferences";
import { canAuthorAtom } from "@/plan/state/atoms";
import { AnnouncerRegions, ChatAnnouncer } from "./announcer";
import { useAttention } from "./attention";
import { BriefDialogHost } from "./brief-dialog";
import { ChatGlyph, STATE_NAME } from "./chat-state-glyph";
import { PendingChips } from "./chips";
import { ComposerArea } from "./composer";
import { ConnectionBanner, useConnectionDim } from "./connection-banner";
import { EmptyChat } from "./empty-chat";
import { pendingPermissionsAtom, sessionAtom, useChatSocket } from "./model";
import { offerErrorAtom } from "./offer-error";
import { createOfferStarter } from "./offer-flow";
import { registerAuthoringOffers } from "./offer-handlers";
import { PanelHead } from "./panel-head";
import { panelWidthAtom } from "./prefs";
import { RunHeader } from "./run-header";
import { ScreenSync } from "./screen-sync";
import { elsewhereSessionAtom, sessionHereAtom } from "./session-home";
import { chatSheetOpenAtom, DESKTOP } from "./sheet-state";
import { shownSessionAtom } from "./shown-session";
import { useQueueRefill } from "./stop-control";
import { useComposerShortcut } from "./composer-hooks";
import { useSuggestionFeed } from "./suggestion-cards";
import { Transcript } from "./transcript";
import { Untrusted } from "./visible-controls";
import "./chat.css";
import "./chat-surface.css";

/** The chat socket and attention watcher for routes that mount no ChatPanel, which owns them there. */
function ChatConnection(): null {
  const store = useStore();
  useChatSocket(store);
  useAttention();
  return null;
}

/**
 * Mounts once beside the routed page so offers, Send to author, live review, the announcements
 * and the Brief dialog work at every width, with the sheet closed.
 * `connect` also opens the chat socket and attention watcher, for routes without a ChatPanel.
 * `panel` is false where the page mounts no ChatPanel: the `c` key then has no composer to focus.
 */
export function ChatRegistrations({
  connect = false,
  panel = true,
}: {
  connect?: boolean;
  panel?: boolean;
}): JSX.Element {
  const store = useStore();
  const desktop = useMediaQuery(DESKTOP);
  const sheetOpen = useAtomValue(chatSheetOpenAtom);
  useReviewLive();
  useSuggestionFeed();
  useQueueRefill();
  useComposerShortcut(panel);
  useEffect(() => {
    const starter = createOfferStarter(store);
    const unregisterOffers = registerAuthoringOffers(({ offer, feature, text }) =>
      starter.start(offer, feature, text),
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
      <ScreenSync />
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
    <p role="alert" className="chat-offer-error">
      <span className="mr-auto">
        <Untrusted text={error} />
      </span>
      <button type="button" className="chat-link-button" onClick={() => setError(null)}>
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

/** A session this screen does not own: the empty state, and the way to where it is. */
function ElsewhereBody({ sheet }: { sheet: boolean }) {
  const elsewhere = useAtomValue(elsewhereSessionAtom);
  const canAuthor = useAtomValue(canAuthorAtom);
  return (
    <div className="chat-panel-top">
      <PanelHead sheet={sheet} />
      {elsewhere ? (
        <p className="chat-elsewhere">
          <Link to={elsewhere.href}>
            A session in <Untrusted text={elsewhere.title} /> needs you
          </Link>
        </p>
      ) : (
        canAuthor && (
          // Attaching is allowed while no chat is open; the chip would otherwise stay unseen.
          <div className="chat-pending-chips">
            <PendingChips />
          </div>
        )
      )}
      <OfferError />
      <ConnectionBanner />
    </div>
  );
}

function SessionBody({ sheet }: { sheet: boolean }) {
  const session = useAtomValue(sessionAtom);
  const shown = useAtomValue(shownSessionAtom);
  const dim = useConnectionDim();
  return (
    <>
      <div className="chat-panel-top">
        <PanelHead sheet={sheet} />
        <OfferError />
        {session && <RunHeader session={session} />}
        <ConnectionBanner />
      </div>
      <div className="chat-panel-middle">
        {dim && <p className="chat-last-update">{dim}</p>}
        {shown ? <Transcript /> : <EmptyChat />}
      </div>
      <ComposerArea />
    </>
  );
}

function PanelBody({ sheet }: { sheet: boolean }) {
  const here = useAtomValue(sessionHereAtom);
  return here ? <SessionBody sheet={sheet} /> : <ElsewhereBody sheet={sheet} />;
}

/** The portal leaves the shell's `[data-loom-plan]` root, which defines the reading face the
 * assistant's prose is set in, so the sheet carries the root's attributes itself. */
function ChatSheet() {
  const [open, setOpen] = useAtom(chatSheetOpenAtom);
  const face = useAtomValue(readingFaceAtom);
  const size = useAtomValue(readingSizeAtom);
  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-(--dialog-backdrop)" />
        <Dialog.Content
          data-loom-plan
          data-reading-face={face}
          data-reading-size={String(size)}
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
  const width = useAtomValue(panelWidthAtom);
  return (
    <>
      <ScreenSync />
      {!desktop ? (
        <ChatSheet />
      ) : (
        <aside
          aria-label="Chat"
          className="chat-panel"
          style={{ width: `${width}rem`, flexShrink: 0 }}
        >
          <ResizeHandle />
          <PanelBody sheet={false} />
        </aside>
      )}
    </>
  );
}
