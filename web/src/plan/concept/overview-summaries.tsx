import { useAtomValue } from "jotai/react";
import { CircleCheck, CircleDashed, CircleX } from "lucide-react";
import type { JSX, ReactNode } from "react";

import { parsePageKey, VISION_DOC } from "@/plan/api/page-key";
import type { ConceptPage } from "@/plan/api/schema-concept";
import { canAuthorAtom } from "@/plan/state/atoms";
import { ApprovalReason, FailureAlert, useAnswerApproval } from "@/plan/summaries/approval-notice";
import { effectiveStatus, viewOf, wordsFor } from "@/plan/summaries/build-notice";
import { buildStatusOf } from "@/plan/summaries/build-status";
import { RebuildButton } from "@/plan/summaries/rebuild-button";
import { summaryPanelAtom } from "@/plan/summaries/state";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { ArrowSpinner } from "@/plan/ui/spinner";

import { summaryIdOfKey } from "./column-context";
import {
  LedgerDetail,
  LedgerProgress,
  LedgerRow,
  LedgerStatus,
  type LedgerTone,
} from "./header-ledger";
import { outOfDateText } from "./summary-text";

/** "1 summary", "7 summaries": `plural` only adds an s. */
function summaryCount(count: number): string {
  return count === 1 ? "1 summary" : `${count} summaries`;
}

/**
 * A rebuild above the budget waits for an answer: the count as the status, the reason and the
 * estimate under it, and for the browser that may answer, "Rebuild all" with "Not now" beside it.
 * A reader sees the words alone.
 */
function ApprovalRow({ label }: { label: string }): JSX.Element | null {
  const { question, canAnswer, onAnswer, failure } = useAnswerApproval();
  if (question === null) return null;
  const action = canAnswer ? (
    <span role="group" aria-label="Rebuild approval" className="header-ledger__buttons">
      <DisabledReason reason={null} reasonPlacement="inline" onClick={() => onAnswer(true)}>
        Rebuild all
      </DisabledReason>
      <DisabledReason
        reason={null}
        look="quiet"
        reasonPlacement="inline"
        onClick={() => onAnswer(false)}
      >
        Not now
      </DisabledReason>
      <FailureAlert failure={failure} />
    </span>
  ) : null;
  return (
    <LedgerRow label={label} action={action}>
      <LedgerStatus tone="warning" glyph={<CircleDashed />}>
        {summaryCount(question.count)} out of date
      </LedgerStatus>
      <LedgerDetail>
        <ApprovalReason question={question} />
      </LedgerDetail>
    </LedgerRow>
  );
}

/**
 * What a row speaks for: the whole overview (its sources and itself) or one summary page. `stale`
 * is what it reports out of date, `detail` the line under a running rebuild, `staleDetail` the
 * line under an out-of-date status.
 */
type Scope = {
  label: string;
  ids: string[];
  plural: boolean;
  stale: { count: number; ids: string[]; words: string };
  upToDate: string;
  detail?: string;
  staleDetail?: string;
  /** A current summary still offers a quiet Rebuild, as a summary page always has. */
  rebuildCurrent: boolean;
};

function overviewScope(page: ConceptPage): Scope {
  const sources = page.key_chips
    .filter((chip) => chip.out_of_date !== null)
    .map((chip) => summaryIdOfKey(chip.doc.page));
  const own = page.state !== null && ["stale", "incomplete"].includes(page.state.state);
  const ids = own ? [...sources, summaryIdOfKey(page.key)] : sources;
  const words = `${summaryCount(ids.length)} out of date`;
  return {
    label: "Summaries",
    ids: [summaryIdOfKey(page.key), ...page.key_chips.map((c) => summaryIdOfKey(c.doc.page))],
    plural: true,
    stale: { count: ids.length, ids, words },
    upToDate: "Summaries up to date",
    detail: ids.length > 0 ? `${words}.` : undefined,
    rebuildCurrent: false,
  };
}

/** One summary page's own freshness: its page state, never its sources' or the project's. */
function pageScope(page: ConceptPage): Scope {
  const state = page.state;
  const own = summaryIdOfKey(page.key);
  const stale = state !== null && (state.out_of_date !== null || state.state === "stale");
  return {
    label: "Summary",
    ids: [own],
    plural: false,
    stale: { count: stale ? 1 : 0, ids: [own], words: "Summary out of date" },
    upToDate: "Summary up to date",
    staleDetail: state?.out_of_date ? outOfDateText(state.out_of_date) : undefined,
    rebuildCurrent: true,
  };
}

function Row({
  label,
  tone,
  glyph,
  words,
  detail,
  action,
  progress,
}: {
  label: string;
  tone: LedgerTone;
  glyph: ReactNode;
  words: ReactNode;
  detail?: ReactNode;
  action?: ReactNode;
  progress?: ReactNode;
}): JSX.Element {
  return (
    <LedgerRow label={label} action={action}>
      <LedgerStatus tone={tone} glyph={glyph} role="status">
        {words}
      </LedgerStatus>
      {detail != null && <LedgerDetail>{detail}</LedgerDetail>}
      {progress}
    </LedgerRow>
  );
}

/** The summaries by what is known: the panel's build status, else the page's own words. */
function SettledRow({ page, scope }: { page: ConceptPage; scope: Scope }): JSX.Element {
  const panel = useAtomValue(summaryPanelAtom);
  const canAuthor = useAtomValue(canAuthorAtom);
  const { label, stale } = scope;
  const status = effectiveStatus(buildStatusOf(panel, scope.ids), {
    claimed: page.state?.state === "rebuilding",
    building: false,
    progress: page.building,
  });
  if (status.kind === "running") {
    const counted = status.total > 0;
    return (
      <Row
        label={label}
        tone="executing"
        glyph={<ArrowSpinner />}
        words={counted ? `Rebuilding ${status.done} of ${status.total}` : "Rebuilding"}
        detail={scope.detail}
        progress={
          counted && (
            <LedgerProgress done={status.done} total={status.total} label="Summaries rebuilt" />
          )
        }
      />
    );
  }
  if (status.kind === "queued" || status.kind === "waiting" || status.kind === "failed") {
    const view = viewOf(status, stale.ids, wordsFor(scope.plural), { title: "" });
    const failed = status.kind === "failed";
    return (
      <Row
        label={label}
        tone={failed ? "blocked" : "pending"}
        glyph={failed ? <CircleX /> : <CircleDashed />}
        words={view.title}
        detail={view.description}
        action={canAuthor ? view.action : null}
      />
    );
  }
  const rebuild = (look: "secondary" | "quiet") =>
    canAuthor ? <RebuildButton ids={stale.ids} look={look} label="Rebuild" /> : null;
  if (stale.count > 0) {
    return (
      <Row
        label={label}
        tone="warning"
        glyph={<CircleDashed />}
        words={stale.words}
        detail={scope.staleDetail}
        action={rebuild("secondary")}
      />
    );
  }
  return (
    <Row
      label={label}
      tone="completed"
      glyph={<CircleCheck />}
      words={scope.upToDate}
      action={scope.rebuildCurrent && page.path !== null ? rebuild("quiet") : null}
    />
  );
}

/**
 * The product overview's Summaries row. An overview with no summary yet draws none: its body's
 * build notice (`empty-states.tsx`) owns that state and its approval question.
 */
export function OverviewSummariesRow({ page }: { page: ConceptPage }): JSX.Element | null {
  const { question } = useAnswerApproval();
  if (page.empty !== null) return null;
  const scope = overviewScope(page);
  if (question !== null) return <ApprovalRow label={scope.label} />;
  return <SettledRow page={page} scope={scope} />;
}

/**
 * Whether the page's summary freshness is a row of its ledger (`PageSummariesRow`) and so draws no
 * status chip: the metaplan and the vision, with a summary state to report.
 */
export function summaryInLedger(page: ConceptPage): boolean {
  const parts = parsePageKey(page.key);
  const own =
    parts?.kind === "summary" &&
    (parts.scope === "metaplan" || (parts.scope === "product" && parts.doc === VISION_DOC));
  return own && page.empty === null && page.state !== null;
}

/**
 * A summary page's Summary row, the metaplan's and the vision's: this page's own freshness, or the
 * project-wide rebuild question while one waits, as the overview draws it. A page without a state
 * or with no summary yet draws none (its heading's status row and its body say so).
 */
export function PageSummariesRow({ page }: { page: ConceptPage }): JSX.Element | null {
  const { question } = useAnswerApproval();
  if (page.empty !== null || page.state === null) return null;
  const scope = pageScope(page);
  if (question !== null) return <ApprovalRow label={scope.label} />;
  return <SettledRow page={page} scope={scope} />;
}
