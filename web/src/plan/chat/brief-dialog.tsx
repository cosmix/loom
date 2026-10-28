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

/** `state` lets a caller that already polls the brief share it; without it the dialog polls. */
export function BriefDialog({
  sessionId,
  state: shared,
  onClose,
}: {
  sessionId: string;
  state?: BriefState;
  onClose: () => void;
}): JSX.Element {
  const own = useSessionBrief(shared ? null : sessionId);
  const state = shared ?? own;
  const returnFocus = useRef<HTMLElement | null>(null);
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent
        className="max-h-[85vh] overflow-y-auto sm:max-w-2xl"
        aria-describedby={undefined}
        onOpenAutoFocus={() => {
          returnFocus.current =
            document.activeElement instanceof HTMLElement ? document.activeElement : null;
        }}
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          returnFocus.current?.focus();
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
