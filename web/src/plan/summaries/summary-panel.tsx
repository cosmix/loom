import { useAtomValue } from "jotai/react";
import { useId, useState, type ReactNode } from "react";

import { QuotaMeters } from "@/components/quota-meters";
import type {
  RunningRebuild,
  SummaryPanel as SummaryPanelData,
  TokenVector,
} from "@/plan/api/schema-summaries";
import { Untrusted } from "@/plan/chat/visible-controls";
import { authorBlockReasonAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { useSummaryActions } from "./state";

export function SummaryPanel({ panel, nowSecs }: { panel: SummaryPanelData; nowSecs: number }) {
  return (
    <div className="w-full min-w-0 max-w-full space-y-4 text-[13px] [overflow-wrap:anywhere]">
      {panel.read_only !== null && (
        <p className="text-muted-foreground">
          <Untrusted text={panel.read_only} />
        </p>
      )}
      <RebuildActivity panel={panel} />
      <Section title="Last rebuild">
        <TokenFigures tokens={panel.last?.tokens} />
      </Section>
      <Section title="Token totals">
        <dl className="space-y-2">
          <div>
            <dt className="text-muted-foreground">Today</dt>
            <dd>
              <TokenFigures tokens={panel.today} />
            </dd>
          </div>
          <div>
            <dt className="text-muted-foreground">7 days</dt>
            <dd>
              <TokenFigures tokens={panel.week} />
            </dd>
          </div>
        </dl>
      </Section>
      <QuotaMeters snapshot={panel.quota} nowSecs={nowSecs} />
      {panel.terminal_claims.length > 0 && (
        <Section title="Terminal claims" className="space-y-2">
          <ul className="space-y-3">
            {panel.terminal_claims.map((id) => (
              <TerminalClaim key={id} id={id} canRelease={panel.read_only === null} />
            ))}
          </ul>
        </Section>
      )}
    </div>
  );
}

/** A region named by its own heading, so the name cannot drift from the visible title. */
function Section({
  title,
  className = "space-y-1",
  children,
}: {
  title: string;
  className?: string;
  children: ReactNode;
}) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className={className}>
      <h3 id={headingId} className="font-medium">
        {title}
      </h3>
      {children}
    </section>
  );
}

/**
 * A Claude session has a tmux terminal to attach to, which this panel does not offer; a Codex job
 * has none, so its log file is the way to follow it.
 */
function RunningRebuildSection({ running }: { running: RunningRebuild }) {
  return (
    <section aria-label="Running rebuild" className="space-y-1">
      <h3 className="font-medium">
        Rebuilding {running.done} of {running.total}
      </h3>
      <ul className="space-y-1 font-mono text-xs">
        {running.ids.map((id) => (
          <li key={id}>
            <Untrusted text={id} />
          </li>
        ))}
      </ul>
      {running.log_path !== null && (
        <p className="text-muted-foreground">
          Log:{" "}
          <code className="font-mono text-xs">
            <Untrusted text={running.log_path} />
          </code>
        </p>
      )}
    </section>
  );
}

function RebuildActivity({ panel }: { panel: SummaryPanelData }) {
  return (
    <>
      {panel.running !== null && <RunningRebuildSection running={panel.running} />}
      {panel.queue.length > 0 && (
        <Section title="Queue">
          <ul className="space-y-1 font-mono text-xs">
            {panel.queue.map((id) => (
              <li key={id}>
                <Untrusted text={id} />
              </li>
            ))}
          </ul>
        </Section>
      )}
      {panel.failed.length > 0 && (
        <Section title="Failed rebuilds">
          <ul className="space-y-2">
            {panel.failed.map(({ id, error }) => (
              <li key={id}>
                <span className="font-mono text-xs">
                  <Untrusted text={id} />
                </span>
                <p className="text-muted-foreground">
                  <Untrusted text={error} />
                </p>
              </li>
            ))}
          </ul>
        </Section>
      )}
      {panel.start_error !== null && (
        <p role="alert" className="text-destructive">
          <Untrusted text={panel.start_error} />
        </p>
      )}
    </>
  );
}

function TokenFigures({ tokens }: { tokens: TokenVector | undefined }) {
  const figure = (value: number | undefined) => value?.toLocaleString("en-US") ?? "unknown";
  return (
    <p className="tabular-nums">
      {`fresh ${figure(tokens?.fresh_input)} · cache reads ${figure(tokens?.cache_read)} · output ${figure(tokens?.output)}`}
    </p>
  );
}

function TerminalClaim({ id, canRelease }: { id: string; canRelease: boolean }) {
  const reason = useAtomValue(authorBlockReasonAtom);
  const { release } = useSummaryActions();
  const [error, setError] = useState<string | null>(null);

  return (
    <li className="min-w-0 space-y-1">
      <div className="flex min-w-0 flex-wrap items-start justify-between gap-2">
        <span className="min-w-0 font-mono text-xs">
          <Untrusted text={id} />
        </span>
        {canRelease && (
          <DisabledReason
            reason={reason}
            size="sm"
            onClick={() => {
              setError(null);
              void release([id]).then(setError);
            }}
          >
            Release
          </DisabledReason>
        )}
      </div>
      {canRelease && error !== null && (
        <p role="alert" className="text-destructive">
          <Untrusted text={error} />
        </p>
      )}
    </li>
  );
}
