import { cn } from "cn";
import { useAtomValue } from "jotai/react";
import { ExternalLinkIcon } from "lucide-react";
import { useCallback, useState, type KeyboardEvent, type ReactNode } from "react";
import { Link, useSearchParams } from "react-router";

import type { Attention, StageSummary } from "@/api/schema";
import { HazardHeader } from "@/aurora-ui/feedback/HazardPanel";
import { OperatorGuidance, attentionCounts, attentionHazard } from "@/components/attention-panel";
import { CopyCommand } from "@/components/copy-command";
import { StageSummaryText } from "@/components/stage-detail";
import { StateLine, ThreadRows } from "@/components/stage-heading";
import { stageHref } from "@/components/stage-href";
import { StageSectionGrid } from "@/components/stage-sections";
import { toneClass } from "@/components/state-badge";
import { useScrollGutter } from "@/components/use-scroll-gutter";
import { HistoryView } from "@/components/terminal/history-view";
import { TerminalMark, terminalGate, terminalHistory } from "@/components/terminal/terminal-glyph";
import {
  IDLE_FRAME,
  TerminalFrameContext,
  TerminalView,
  type TerminalFrameState,
} from "@/components/terminal/terminal-view";
import type { EmulatorFactory, TerminalDeps } from "@/components/terminal/use-terminal";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Kbd } from "@/components/ui/kbd";
import type { HistoryLoader } from "@/api/terminal-history";
import { stateMeta } from "@/lib/format";
import { attentionAtom, orderedStagesAtom, selectStage, snapshotAtom } from "@/state/atoms";

/// The query parameter naming the stage the modal shows, so an open modal
/// has a URL and the back button closes it.
export const STAGE_PARAM = "stage";
/// Which face of the stage the modal shows; absent means the details.
export const STAGE_VIEW_PARAM = "view";
const TERMINAL_VIEW = "terminal";

export function useOpenStage(): (id: string) => void {
  const [, setParams] = useSearchParams();
  return useCallback(
    (id: string) =>
      setParams((params) => {
        const next = new URLSearchParams(params);
        next.set(STAGE_PARAM, id);
        return next;
      }),
    [setParams],
  );
}

/// Opens the stage's terminal view: `?stage=<id>&view=terminal`. Back returns
/// to the details, back again closes.
export function useOpenTerminal(): (id: string) => void {
  const [, setParams] = useSearchParams();
  return useCallback(
    (id: string) =>
      setParams((params) => {
        const next = new URLSearchParams(params);
        next.set(STAGE_PARAM, id);
        next.set(STAGE_VIEW_PARAM, TERMINAL_VIEW);
        return next;
      }),
    [setParams],
  );
}

function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
}

/// The stage page's content in a dialog, opened from any view with
/// `?stage=<id>`; upstream and downstream chips move the dialog along the
/// thread without closing it. With `&view=terminal` the dialog becomes the
/// window onto the agent: the back arrow returns to the details in either
/// mode, and Esc does too in view mode; in control mode Esc goes to the agent,
/// and only the close button or the browser's back button leave.
export interface StageModalProps {
  terminalFactory?: EmulatorFactory;
  terminalDeps?: TerminalDeps;
  historyFactory?: EmulatorFactory;
  historyLoader?: HistoryLoader;
}

interface StageView {
  stage: StageSummary | undefined;
  gate: string | null;
  history: string | null;
  nativeBackend: boolean;
}

/// The stage behind `?stage=<id>` and what the terminal may do for it.
function useStageView(id: string | null): StageView {
  const snapshot = useAtomValue(snapshotAtom);
  const stage = id === null ? undefined : selectStage(snapshot, id);
  const enabled = snapshot?.terminals ?? false;
  return {
    stage,
    gate: stage === undefined ? null : terminalGate(enabled, stage),
    history: stage === undefined ? null : terminalHistory(enabled, stage),
    nativeBackend: stage !== undefined && enabled && stage.session_backend === "native",
  };
}

/// Closes the modal, or returns from the terminal to the details.
function useStageNav(): { close: () => void; toDetails: () => void } {
  const [, setParams] = useSearchParams();
  const edit = (drop: string[]) =>
    setParams((current) => {
      const next = new URLSearchParams(current);
      drop.forEach((key) => next.delete(key));
      return next;
    });
  return {
    close: () => edit([STAGE_PARAM, STAGE_VIEW_PARAM]),
    toDetails: () => edit([STAGE_VIEW_PARAM]),
  };
}

/// The `t` shortcut: opens the terminal (or its history) from the details.
function useTerminalShortcut(
  view: StageView,
  showTerminal: boolean,
): (event: KeyboardEvent<HTMLDivElement>) => void {
  const openTerminal = useOpenTerminal();
  const { stage, gate, history } = view;
  return (event) => {
    if (showTerminal || event.key !== "t" || event.altKey || event.ctrlKey || event.metaKey) return;
    if (isTyping(event.target) || stage === undefined) return;
    if (gate !== null && history === null) return;
    event.preventDefault();
    openTerminal(stage.id);
  };
}

export function StageModal(props: StageModalProps = {}) {
  const [params] = useSearchParams();
  const id = params.get(STAGE_PARAM);
  const terminal = params.get(STAGE_VIEW_PARAM) === TERMINAL_VIEW;
  const view = useStageView(id);
  const { close, toDetails } = useStageNav();
  const [frame, setFrame] = useState<TerminalFrameState>(IDLE_FRAME);
  const showTerminal = terminal && view.stage !== undefined;
  const onKeyDown = useTerminalShortcut(view, showTerminal);
  const guard = (event: Event) => {
    if (showTerminal && frame.mode === "control") event.preventDefault();
  };

  return (
    <Dialog
      open={id !== null}
      onOpenChange={(open) => {
        if (!open) close();
      }}
    >
      <DialogContent
        className={cn(
          "stage-modal gap-0 overflow-hidden p-0",
          showTerminal
            ? "terminal-dialog h-[min(86dvh,860px)] grid-rows-[auto_1fr_auto] sm:max-w-[min(96vw,1280px)]"
            : "sm:max-w-[min(92vw,1100px)]",
        )}
        data-mode={showTerminal ? frame.mode : undefined}
        data-phase={showTerminal ? frame.phase : undefined}
        onKeyDown={onKeyDown}
        onEscapeKeyDown={(event) => {
          if (!showTerminal) return;
          event.preventDefault();
          if (frame.mode === "view") toDetails();
        }}
        onInteractOutside={guard}
        onPointerDownOutside={guard}
      >
        <Panes
          id={id}
          terminal={terminal}
          view={view}
          toDetails={toDetails}
          setFrame={setFrame}
          {...props}
        />
      </DialogContent>
    </Dialog>
  );
}

/// What fills the dialog: the missing notice, details, saved screen or terminal.
function Panes({
  id,
  terminal,
  view,
  toDetails,
  setFrame,
  ...props
}: StageModalProps & {
  id: string | null;
  terminal: boolean;
  view: StageView;
  toDetails: () => void;
  setFrame: (state: TerminalFrameState) => void;
}) {
  const { stage, gate, history, nativeBackend } = view;
  if (id !== null && stage === undefined) return <Missing id={id} />;
  if (!stage) return null;
  if (!terminal) {
    return <Body stage={stage} gate={gate} history={history} nativeBackend={nativeBackend} />;
  }
  if (history !== null) {
    return (
      <HistoryView
        stage={stage}
        session={history}
        frame="dialog"
        factory={props.historyFactory}
        load={props.historyLoader}
        onBack={toDetails}
      />
    );
  }
  return (
    <TerminalFrameContext.Provider value={setFrame}>
      <TerminalView
        stage={stage}
        frame="dialog"
        factory={props.terminalFactory}
        deps={props.terminalDeps}
        onBack={toDetails}
      />
    </TerminalFrameContext.Provider>
  );
}

function Body({
  stage,
  gate,
  history,
  nativeBackend,
}: {
  stage: StageSummary;
  gate: string | null;
  history: string | null;
  nativeBackend: boolean;
}) {
  const openTerminal = useOpenTerminal();
  const ordered = useAtomValue(orderedStagesAtom);
  const attention = useAtomValue(attentionAtom).find((entry) => entry.id === stage.id);
  const level = ordered.find((entry) => entry.stage.id === stage.id)?.level ?? null;
  const scroller = useScrollGutter<HTMLDivElement>();

  return (
    <>
      {attention && <AttentionBanner attention={attention} />}
      <BodyHead
        stage={stage}
        terminalButton={
          <TerminalButton
            reason={history === null ? gate : null}
            history={history !== null}
            nativeBackend={nativeBackend}
            onOpen={() => openTerminal(stage.id)}
          />
        }
      />
      <div
        ref={scroller}
        className="max-h-[62dvh] overflow-y-auto p-5 pr-[max(0px,calc(1.25rem-var(--gutter,0px)))]"
      >
        <div className="flex flex-col gap-4">
          {attention && <OperatorGuidance entry={attention} />}
          <StageSectionGrid stage={stage} level={level} />
        </div>
      </div>
      <BodyFoot id={stage.id} />
    </>
  );
}

function AttentionBanner({ attention }: { attention: Attention }) {
  const counts = attentionCounts(attention);
  return (
    <HazardHeader
      tone={attentionHazard(attention)}
      title={attention.label}
      className="pr-12 sm:pr-12"
      action={counts && <span className="text-xs text-muted-foreground">{counts}</span>}
    />
  );
}

function BodyHead({ stage, terminalButton }: { stage: StageSummary; terminalButton: ReactNode }) {
  const open = useOpenStage();
  return (
    <DialogHeader
      className={cn("stage-modal-head gap-2 p-5", toneClass(stateMeta(stage.status).tone))}
    >
      {/* Only the title row clears the dialog's close button, so everything
          below keeps the body's right edge. */}
      <div className="flex items-start gap-x-3 gap-y-1 pr-7">
        <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1.5">
          <DialogTitle className="text-xl leading-tight font-semibold tracking-tight text-balance text-foreground">
            {stage.name}
          </DialogTitle>
          <span className="stage-id-chip">{stage.id}</span>
        </div>
        {terminalButton}
      </div>
      <StageSummaryText summary={stage.summary} />
      <DialogDescription asChild>
        <StateLine stage={stage} className="stage-state-strip text-foreground" />
      </DialogDescription>
      <ThreadRows stage={stage} pick={open} className="mt-2" />
    </DialogHeader>
  );
}

function BodyFoot({ id }: { id: string }) {
  return (
    <footer className="flex items-center justify-between gap-3 border-t border-hairline bg-muted/40 px-5 py-2.5 text-xs text-muted-foreground">
      <span>
        <Kbd>Esc</Kbd> closes
      </span>
      <Link
        to={stageHref(id)}
        className="inline-flex items-center gap-1.5 text-foreground hover:underline"
      >
        open as a page
        <ExternalLinkIcon className="size-3.5" />
      </Link>
    </footer>
  );
}

/// The primary way into the terminal.
function TerminalButton({
  reason,
  history,
  nativeBackend,
  onOpen,
}: {
  reason: string | null;
  history: boolean;
  nativeBackend: boolean;
  onOpen: () => void;
}) {
  return (
    <div className="ml-auto flex shrink-0 items-center gap-2">
      <Button
        type="button"
        variant="outline"
        size="sm"
        disabled={reason !== null}
        onClick={onOpen}
        aria-label={history ? "open terminal history" : "open terminal"}
        title={
          history ? "The session ended; its saved screen opens read-only" : (reason ?? undefined)
        }
      >
        <TerminalMark className="size-3.5" />
        {history ? "Terminal history" : "Terminal"}
        <Kbd className="h-4 min-w-4 px-1 text-[10px]">t</Kbd>
      </Button>
      {nativeBackend && <CopyCommand command="loom run --backend tmux" />}
    </div>
  );
}

function Missing({ id }: { id: string }) {
  return (
    <DialogHeader className="p-5">
      <DialogTitle>No such stage</DialogTitle>
      <DialogDescription>
        No stage named <code className="rounded bg-muted px-1 py-0.5 text-xs">{id}</code> in this
        workspace.
      </DialogDescription>
    </DialogHeader>
  );
}
