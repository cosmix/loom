import { cn } from "cn";
import { ExternalLinkIcon } from "lucide-react";
import { useEffect, useRef, useState, type ReactElement } from "react";
import { Link } from "react-router";

import { fetchTerminalHistory, type HistoryLoader } from "@/api/terminal-history";
import type { StageSummary } from "@/api/schema";
import { CopyCommand } from "@/components/copy-command";
import { createHistoryEmulator } from "@/components/terminal/emulator";
import { BackButton, Line, Name, rise, terminalHref } from "@/components/terminal/terminal-view";
import type { EmulatorFactory } from "@/components/terminal/use-terminal";
import { Kbd } from "@/components/ui/kbd";
import { toneClass } from "@/components/state-badge";
import { stateMeta } from "@/lib/format";

export interface HistoryViewProps {
  stage: StageSummary;
  /// The ended session whose saved screen this shows.
  session: string;
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

const NOTICE: Record<"missing" | "failed", string> = {
  missing: "This session left no saved terminal history.",
  failed: "The saved terminal history could not be loaded.",
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
  frame,
  factory = createHistoryEmulator,
  load = fetchTerminalHistory,
  onBack,
}: HistoryViewProps): ReactElement {
  const { hostRef, phase } = useSavedScreen(session, factory, load);

  return (
    <div
      className={cn("terminal-frame", toneClass(stateMeta(stage.status).tone))}
      data-frame={frame}
      data-mode="view"
      data-phase={WELL_PHASE[phase]}
    >
      <HistoryHead stage={stage} frame={frame} onBack={onBack} />
      <div className="terminal-well-wrap">
        <div className="terminal-well" data-mode="view" data-phase={WELL_PHASE[phase]}>
          <div ref={hostRef} className="terminal-host" aria-label="Saved terminal history" />
          {(phase === "missing" || phase === "failed") && (
            <div className="terminal-notice" role="alert">
              <span className="line">{NOTICE[phase]}</span>
            </div>
          )}
        </div>
      </div>
      <HistoryFoot stage={stage} session={session} frame={frame} />
    </div>
  );
}

function HistoryHead({
  stage,
  frame,
  onBack,
}: {
  stage: StageSummary;
  frame: HistoryViewProps["frame"];
  onBack?: () => void;
}) {
  return (
    <header className="terminal-head">
      <div className="flex min-w-0 flex-col gap-1.5">
        <div className="rise flex items-center gap-2" style={rise(0)}>
          {onBack && <BackButton onBack={onBack} />}
          <span className="eyebrow">Terminal history</span>
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
  frame,
}: {
  stage: StageSummary;
  session: string;
  frame: HistoryViewProps["frame"];
}) {
  return (
    <footer className="terminal-foot">
      <span className="terminal-hint">
        read-only · the session ended · this is its saved screen
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
