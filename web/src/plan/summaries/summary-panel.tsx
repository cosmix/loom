import { useAtomValue } from "jotai/react";
import { useId, useState, type ReactNode } from "react";

import type {
  RunningRebuild,
  SummaryPanel as SummaryPanelData,
  TokenVector,
} from "@/plan/api/schema-summaries";
import { Untrusted } from "@/plan/chat/visible-controls";
import { relativeTime } from "@/plan/files/heading-parts";
import { authorBlockReasonAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { useSummaryActions } from "./state";

/**
 * The rebuild statistics, mounted only by Settings. The provider limits are not drawn here: the
 * footer is their one place.
 */
export function SummaryPanel({ panel, nowSecs }: { panel: SummaryPanelData; nowSecs: number }) {
  return (
    <div className="w-full min-w-0 max-w-full space-y-[var(--space-5)] text-[length:var(--type-ui)] [overflow-wrap:anywhere]">
      {panel.read_only !== null && (
        <p className="text-muted-foreground">
          <Untrusted text={panel.read_only} />
        </p>
      )}
      <RebuildActivity panel={panel} />
      <TokenStatistics panel={panel} nowSecs={nowSecs} />
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
      <h3 id={headingId} className="font-semibold text-foreground">
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
      <h3 className="font-semibold text-foreground">
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

/**
 * The ids a pending question already lists (rebuild-question.tsx, drawn above this panel) are
 * left out of the queue, so each id shows once. Ids queued for another reason stay.
 */
function queueBesideQuestion(panel: SummaryPanelData): string[] {
  const asked = new Set(panel.question?.ids ?? []);
  return panel.queue.filter((id) => !asked.has(id));
}

/** The running rebuild, the queue, failed rebuilds and the start error. */
export function RebuildActivity({ panel }: { panel: SummaryPanelData }) {
  const queue = queueBesideQuestion(panel);
  return (
    <>
      {panel.running !== null && <RunningRebuildSection running={panel.running} />}
      {queue.length > 0 && (
        <Section title="Queue">
          <ul className="space-y-1 font-mono text-xs">
            {queue.map((id) => (
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

const COLUMN = "grid min-w-0 content-start gap-[var(--space-2)]";
const CAPTION = "font-medium text-foreground";

/**
 * Last rebuild, Today and 7 days as three equal columns of labelled figures, each under a caption
 * naming its span, so a figure reads across the row against its neighbours. Below `md` the last
 * rebuild takes the first of two columns and the totals the row under it, so each column keeps room
 * for "cache reads 412,000" and the 390px dialog never scrolls sideways. The two sections' headings
 * and captions share one size and gap, which keeps the figure rows on one line without a subgrid.
 */
function TokenStatistics({ panel, nowSecs }: { panel: SummaryPanelData; nowSecs: number }) {
  return (
    <div className="grid grid-cols-2 gap-x-[var(--space-4)] gap-y-[var(--space-5)] md:grid-cols-[minmax(0,13rem)_minmax(0,27rem)]">
      <Section title="Last rebuild" className={COLUMN}>
        <p className={CAPTION}>
          {panel.last === null ? (
            "None yet"
          ) : (
            <time dateTime={panel.last.time}>{relativeTime(panel.last.time, nowSecs * 1000)}</time>
          )}
        </p>
        <TokenFigures tokens={panel.last?.tokens} />
      </Section>
      <Section title="Token totals" className={`${COLUMN} col-span-2 md:col-span-1`}>
        <dl className="grid grid-cols-2 gap-x-[var(--space-4)]">
          <div className={COLUMN}>
            <dt className={CAPTION}>Today</dt>
            <dd>
              <TokenFigures tokens={panel.today} />
            </dd>
          </div>
          <div className={COLUMN}>
            <dt className={CAPTION}>7 days</dt>
            <dd>
              <TokenFigures tokens={panel.week} />
            </dd>
          </div>
        </dl>
      </Section>
    </div>
  );
}

const FIGURES = [
  ["fresh", "fresh_input"],
  ["cache reads", "cache_read"],
  ["output", "output"],
] as const;

/**
 * One labelled figure per line, the figure set flush right in tabular numerals so the digits of a
 * column line up on the units. A missing figure reads "unknown" in muted text; zero prints as 0.
 */
function TokenFigures({ tokens }: { tokens: TokenVector | undefined }) {
  return (
    <dl className="grid gap-[var(--space-1)]">
      {FIGURES.map(([label, key]) => {
        const value = tokens?.[key];
        return (
          <div
            key={key}
            className="flex min-w-0 items-baseline justify-between gap-[var(--space-2)]"
          >
            <dt className="text-muted-foreground">{label}</dt>
            <dd
              className={
                value === undefined ? "text-muted-foreground" : "tabular-nums text-foreground"
              }
            >
              {value?.toLocaleString("en-US") ?? "unknown"}
            </dd>
          </div>
        );
      })}
    </dl>
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
