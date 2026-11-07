import { useAtomValue } from "jotai/react";
import { useId, useRef, useState, type ReactNode } from "react";

import type { RunningRebuild, SummaryPanel as SummaryPanelData } from "@/plan/api/schema-summaries";
import { Untrusted } from "@/plan/chat/visible-controls";
import { relativeTime } from "@/plan/files/heading-parts";
import { authorBlockReasonAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { useSummaryActions } from "./state";
import { COMPONENTS, TokenCards, TokenFigure } from "./token-figures";
import "./summary-panel.css";

type CostLine = NonNullable<SummaryPanelData["last"]>;

/**
 * The rebuild statistics, mounted only by Settings: the token totals as two figure cards, the
 * last rebuild as one row, then what is in progress (`RebuildActivity`, shared with the activity
 * popover) and the terminal claims. The provider limits are not drawn here: the footer is their
 * one place.
 */
export function SummaryPanel({ panel, nowSecs }: { panel: SummaryPanelData; nowSecs: number }) {
  const now = nowSecs * 1000;
  return (
    <div className="sp-panel">
      {panel.read_only !== null && (
        <p className="sp-muted">
          <Untrusted text={panel.read_only} />
        </p>
      )}
      <Section title="Token totals">
        <TokenCards today={panel.today} week={panel.week} />
      </Section>
      <Section title="Last rebuild">
        <LastRebuild last={panel.last} failed={panel.failed.length > 0} now={now} />
      </Section>
      {panel.running === null && <p className="sp-idle">{idleLine(panel.last, now)}</p>}
      <RebuildActivity panel={panel} />
      {panel.terminal_claims.length > 0 && (
        <Section title="Terminal claims">
          <ul className="plan-list sp-rows">
            {panel.terminal_claims.map((id) => (
              <ActionRow
                key={id}
                id={id}
                verb="Release"
                action="release"
                canAct={panel.read_only === null}
              />
            ))}
          </ul>
        </Section>
      )}
    </div>
  );
}

/** A region named by its own heading, so the name cannot drift from the visible title. */
function Section({ title, children }: { title: string; children: ReactNode }) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="sp-section">
      <h3 id={headingId} className="sp-title">
        {title}
      </h3>
      {children}
    </section>
  );
}

function IdList({ ids }: { ids: string[] }) {
  return (
    <ul className="sp-ids">
      {ids.map((id) => (
        <li key={id}>
          <Untrusted text={id} />
        </li>
      ))}
    </ul>
  );
}

function idleLine(last: CostLine | null, now: number): string {
  return last === null ? "Idle · no runs yet" : `Idle · last run ${relativeTime(last.time, now)}`;
}

/** The three components of the last rebuild's cost line; cache writes stay in the cards' legends. */
const COST_COMPONENTS = COMPONENTS.flatMap((component, ramp) =>
  component.key === "cache_write" ? [] : [{ ...component, ramp }],
);

/**
 * The wire carries no duration and no success flag for the last rebuild: the row shows none, and
 * its dot reads "failed" while the panel lists any failed rebuild.
 */
function LastRebuild({
  last,
  failed,
  now,
}: {
  last: CostLine | null;
  failed: boolean;
  now: number;
}) {
  if (last === null) return <p className="sp-muted">None yet</p>;
  const count = last.summaries.length;
  return (
    <div className="sp-last">
      <p className="sp-last-head">
        <span className="sp-last-state">
          <span className="sp-dot" data-state={failed ? "failed" : "ok"} aria-hidden="true" />
          {failed ? "Failed" : "Rebuilt"}
        </span>
        <time
          dateTime={last.time}
          title={new Date(last.time).toLocaleString()}
          className="sp-muted"
        >
          {relativeTime(last.time, now)}
        </time>
        <span className="sp-muted">
          {count} {count === 1 ? "document" : "documents"}
        </span>
      </p>
      <dl className="sp-cost">
        {COST_COMPONENTS.map(({ key, label, ramp }) => (
          <TokenFigure key={key} label={label} value={last.tokens[key]} ramp={ramp} />
        ))}
      </dl>
    </div>
  );
}

/**
 * A Claude session has a tmux terminal to attach to, which this panel does not offer; a Codex job
 * has none, so its log file is the way to follow it.
 */
function RunningRebuildSection({ running }: { running: RunningRebuild }) {
  const share = running.total > 0 ? (running.done / running.total) * 100 : 0;
  return (
    <section aria-label="Running rebuild" className="sp-section">
      <div className="sp-running-head">
        <span className="sp-pulse" aria-hidden="true" />
        <h3 className="sp-title">
          Rebuilding {running.done} of {running.total}
        </h3>
      </div>
      <div
        role="progressbar"
        aria-label="Rebuild progress"
        aria-valuemin={0}
        aria-valuemax={running.total}
        aria-valuenow={running.done}
        className="sp-progress"
      >
        <span className="sp-progress-fill" style={{ width: `${share}%` }} />
      </div>
      <IdList ids={running.ids} />
      {running.log_path !== null && (
        <p className="sp-muted">
          Log:{" "}
          <code className="sp-mono">
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
          <IdList ids={queue} />
        </Section>
      )}
      {panel.failed.length > 0 && (
        <Section title="Failed rebuilds">
          <ul className="plan-list sp-rows">
            {panel.failed.map(({ id, error }) => (
              <ActionRow
                key={id}
                id={id}
                detail={error}
                verb="Retry"
                action="rebuild"
                canAct={panel.read_only === null}
              />
            ))}
          </ul>
        </Section>
      )}
      {panel.start_error !== null && (
        <p role="alert" className="sp-alert">
          <Untrusted text={panel.start_error} />
        </p>
      )}
    </>
  );
}

/**
 * A compact row: the id in mono, a detail under it, and one action at its end (Retry a failed
 * rebuild, Release a terminal claim), drawn only while the panel is not read-only. The action's
 * failure shows as an alert under the id, as RebuildButton's does beside its button.
 */
function ActionRow({
  id,
  detail,
  verb,
  action,
  canAct,
}: {
  id: string;
  detail?: string;
  verb: string;
  action: "rebuild" | "release";
  canAct: boolean;
}) {
  const reason = useAtomValue(authorBlockReasonAtom);
  const actions = useSummaryActions();
  const [error, setError] = useState<string | null>(null);
  const pending = useRef(false);

  async function handleAct() {
    if (pending.current) return;
    pending.current = true;
    setError(null);
    try {
      setError(await actions[action]([id]));
    } finally {
      pending.current = false;
    }
  }

  return (
    <li className="sp-row">
      <div className="sp-row-main">
        <span className="sp-mono">
          <Untrusted text={id} />
        </span>
        {detail !== undefined && (
          <p className="sp-row-sub">
            <Untrusted text={detail} />
          </p>
        )}
        {canAct && error !== null && (
          <p role="alert" className="sp-alert">
            <Untrusted text={error} />
          </p>
        )}
      </div>
      {canAct && (
        <div className="sp-row-action">
          <DisabledReason reason={reason} size="sm" onClick={() => void handleAct()}>
            {verb}
          </DisabledReason>
        </div>
      )}
    </li>
  );
}
