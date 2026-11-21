import { cn } from "cn";
import { ExternalLinkIcon } from "lucide-react";
import { useEffect, useRef, useState, type ReactElement } from "react";
import { Link } from "react-router";

import { fetchTerminalHistory, type HistoryLoader } from "@/api/terminal-history";
import type { StageSummary } from "@/api/schema";
import { CopyCommand } from "@/components/copy-command";
import { createHistoryEmulator } from "@/components/terminal/emulator";
import { historyLabels, type HistoryKind } from "@/components/terminal/history-labels";
import { BackButton, Line, Name, rise, terminalHref } from "@/components/terminal/terminal-view";
import type { EmulatorFactory } from "@/components/terminal/use-terminal";
import { Kbd } from "@/components/ui/kbd";
import { toneClass } from "@/components/state-badge";
import { displayState } from "@/lib/display-state";
import { stateMeta } from "@/lib/format";

export interface HistoryViewProps {
  stage: StageSummary;
  /// The ended session whose saved screen this shows.
  session: string;
  /// What the saved screen is; it words every label of the view.
  kind: HistoryKind;
  frame: "dialog" | "page";
  factory?: EmulatorFactory;
  load?: HistoryLoader;
  /// Returns to the stage details; the header shows a back arrow only when given.
  onBack?: () => void;
}

type Phase = "loading" | "shown" | "missing" | "failed";

/// The `data-phase` the shared terminal styles know: the host stays hidden
/// until the saved screen is written, and refusals get the centred notice.
const WELL_PHASE: Record<Phase, string> = {
  loading: "connecting",
  shown: "history",
  missing: "refused",
  failed: "refused",
};

/// The `less -R` command that reads the saved screen from a shell.
export function historyCommand(session: string): string {
  return `less -R .loom/work/terminal-history/${session}.terminal`;
}

/// Replays the session's saved bytes into a read-only emulator mounted on the
/// returned `hostRef`, and reports whether the replay is shown. Unmounting
/// disposes the emulator and the resize observer.
function useSavedScreen(session: string, factory: EmulatorFactory, load: HistoryLoader) {
  const hostRef = useRef<HTMLDivElement>(null);
  const [phase, setPhase] = useState<Phase>("loading");

  useEffect(() => {
    let cancelled = false;
    let dispose: (() => void) | undefined;
    void Promise.allSettled([factory(), load(session)]).then(([created, loaded]) => {
      const emulator = created.status === "fulfilled" ? created.value : undefined;
      const bytes = loaded.status === "fulfilled" ? loaded.value : undefined;
      const host = hostRef.current;
      if (cancelled || !host || !emulator || !bytes) {
        emulator?.dispose();
        if (!cancelled) setPhase(bytes === null ? "missing" : "failed");
        return;
      }
      emulator.open(host);
      emulator.setReadOnly(true);
      emulator.fit();
      emulator.write(bytes);
      const observer = new ResizeObserver(() => emulator.fit());
      observer.observe(host);
      dispose = () => {
        observer.disconnect();
        emulator.dispose();
      };
      setPhase("shown");
    });
    return () => {
      cancelled = true;
      dispose?.();
    };
  }, [session, factory, load]);

  return { hostRef, phase };
}

/// The last screen and scrollback of a session that has ended, replayed into a
/// read-only terminal: no connection to the session, no input. The saved bytes
/// are untrusted terminal output and go only to the emulator, never into markup.
export function HistoryView({
  stage,
  session,
  kind,
  frame,
  factory = createHistoryEmulator,
  load = fetchTerminalHistory,
  onBack,
}: HistoryViewProps): ReactElement {
  const { hostRef, phase } = useSavedScreen(session, factory, load);
  const labels = historyLabels(kind);

  return (
    <div
      className={cn("terminal-frame", toneClass(stateMeta(displayState(stage)).tone))}
      data-frame={frame}
      data-mode="view"
      data-phase={WELL_PHASE[phase]}
    >
      <HistoryHead stage={stage} eyebrow={labels.eyebrow} frame={frame} onBack={onBack} />
      <div className="terminal-well-wrap">
        <div className="terminal-well" data-mode="view" data-phase={WELL_PHASE[phase]}>
          <div ref={hostRef} className="terminal-host" aria-label={labels.screenAria} />
          {(phase === "missing" || phase === "failed") && (
            <div className="terminal-notice" role="alert">
              <span className="line">{labels[phase]}</span>
            </div>
          )}
        </div>
      </div>
      <HistoryFoot stage={stage} session={session} footer={labels.footer} frame={frame} />
    </div>
  );
}

function HistoryHead({
  stage,
  eyebrow,
  frame,
  onBack,
}: {
  stage: StageSummary;
  eyebrow: string;
  frame: HistoryViewProps["frame"];
  onBack?: () => void;
}) {
  return (
    <header className="terminal-head">
      <div className="flex min-w-0 flex-col gap-1.5">
        <div className="rise flex items-center gap-2" style={rise(0)}>
          {onBack && <BackButton onBack={onBack} />}
          <span className="eyebrow">{eyebrow}</span>
        </div>
        <div className="rise flex flex-wrap items-baseline gap-x-3 gap-y-1" style={rise(1)}>
          <Name stage={stage} frame={frame} />
          <span className="font-mono text-sm text-muted-foreground">{stage.id}</span>
        </div>
        <Line stage={stage} frame={frame} />
      </div>
    </header>
  );
}

function HistoryFoot({
  stage,
  session,
  footer,
  frame,
}: {
  stage: StageSummary;
  session: string;
  footer: string;
  frame: HistoryViewProps["frame"];
}) {
  return (
    <footer className="terminal-foot">
      <span className="terminal-hint">
        {footer}
        {frame === "dialog" && (
          <>
            {" · "}
            <Kbd>Esc</Kbd> back to details
          </>
        )}
      </span>
      <div className="flex items-center gap-4">
        <CopyCommand command={historyCommand(session)} />
        {frame === "dialog" && (
          <Link
            to={terminalHref(stage.id)}
            className="inline-flex items-center gap-1.5 text-foreground hover:underline"
          >
            open in a tab
            <ExternalLinkIcon className="size-3.5" />
          </Link>
        )}
      </div>
    </footer>
  );
}
