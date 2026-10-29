import { atom } from "jotai";
import { useAtom } from "jotai/react";
import { useRef, type JSX } from "react";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { useSessionBrief, type BriefState } from "@/plan/chat/use-session-brief";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { LoadState } from "@/plan/ui/load-state";

function BriefBody({ state }: { state: BriefState }): JSX.Element {
  switch (state.status) {
    case "loading":
      return <LoadState state="loading" title="Loading brief" />;
    case "unavailable":
      return <LoadState state="not_found" title="Brief not available" />;
    case "error":
      return (
        <LoadState
          state="error"
          title="Brief not available"
          message={visibleControls(state.message)}
        />
      );
    case "ready":
      return (
        <div className="whitespace-pre-wrap break-words text-sm leading-normal">
          <Untrusted text={state.brief.brief} />
        </div>
      );
  }
}

/**
 * The session whose brief is open, or null. Held outside the Starting row: that row unmounts the
 * moment the session leaves `starting`, and an open dialog must not vanish with it mid-read.
 */
export const briefOpenAtom = atom<string | null>(null);

/** Renders the open Brief dialog; mounted with the chat registrations so it outlives the row. */
export function BriefDialogHost(): JSX.Element | null {
  const [sessionId, setSessionId] = useAtom(briefOpenAtom);
  return sessionId === null ? null : (
    <BriefDialog sessionId={sessionId} onClose={() => setSessionId(null)} />
  );
}

export function BriefDialog({
  sessionId,
  onClose,
}: {
  sessionId: string;
  onClose: () => void;
}): JSX.Element {
  const state = useSessionBrief(sessionId);
  const returnFocus = useRef<HTMLElement | null>(null);
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent
        className="max-h-[85vh] overflow-y-auto sm:max-w-2xl motion-reduce:animate-none"
        aria-describedby={undefined}
        onOpenAutoFocus={() => {
          returnFocus.current =
            document.activeElement instanceof HTMLElement ? document.activeElement : null;
        }}
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          // The opener (the Brief link) is gone once the session has started; the panel heading
          // is the stable place to land, not the page body.
          const opener = returnFocus.current;
          const stable = opener && opener !== document.body && opener.isConnected;
          (stable ? opener : document.querySelector<HTMLElement>("[data-chat-heading]"))?.focus();
        }}
      >
        <DialogHeader>
          <DialogTitle>Brief</DialogTitle>
        </DialogHeader>
        <BriefBody state={state} />
      </DialogContent>
    </Dialog>
  );
}
