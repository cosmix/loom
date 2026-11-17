import { FoldVertical, UnfoldVertical } from "lucide-react";
import type { JSX, ReactNode } from "react";

import {
  VISION_DOC,
  docKindOfPageKey,
  metaplanKey,
  overviewKey,
  pageHref,
  parsePageKey,
} from "@/plan/api/page-key";
import type { ConceptPage } from "@/plan/api/schema-concept";
import { usePressureRun } from "@/plan/chat/pressure/use-pressure";
import { Untrusted } from "@/plan/chat/visible-controls";
import {
  featureOfDocPath,
  featureSourceHref,
  pressureRecordHref,
} from "@/plan/feature/source-href";
import { repoRel } from "@/plan/files/address";
import {
  builtByChips,
  featureCrumb,
  isRebuilding,
  planStateChip,
  pressureChip,
  sectionLabel,
  sectionNumberLabel,
  slugNumber,
  summaryChips,
  useFeatureFacts,
  useMyCallChips,
  useOpenQuestionChips,
} from "@/plan/files/heading-parts";
import { HeadingActions, usePressureActive } from "@/plan/files/doc-heading-view";
import { LockNotice } from "@/plan/files/lock-notice";
import { PressureTestAction, PressureTestDetail } from "@/plan/run/plan-run-actions";
import { useTraceChips } from "@/plan/trace/heading-chips";
import { withoutNumber } from "@/plan/trace/model";
import { PlanExtras } from "@/plan/trace/plan-extras";
import { ScopeLine } from "@/plan/trace/scope-line";
import { docIsLocked, SettleControl } from "@/plan/trace/settle-control";
import {
  DocHeading,
  HEADING_BUTTON,
  type HeadingChip,
  type HeadingCrumb,
  ToolbarGroup,
} from "@/plan/ui/doc-heading";

import { LedgerRows } from "./header-ledger";
import { PageSummariesRow, summaryInLedger } from "./overview-summaries";
import { hasOpenable, useAllOpen, useFoldAll, useOpenAll } from "./open-all";
import { useKeptDoc } from "./passage-doc";
import { PageHeaderExtras, SummaryAction, SummaryDetail } from "./page-header-extras";

const RUN_NOTICE =
  "This plan is running. `loom run` works from the committed copy, so the app keeps it read-only.";
const DONE_NOTICE = "This plan is done. Record changes in a new plan.";
const LOCKED_NOTICES: Partial<Record<NonNullable<ConceptPage["plan_state"]>, string>> = {
  running: RUN_NOTICE,
  done: DONE_NOTICE,
  retired: DONE_NOTICE,
};

/** Feature slug, then the area, then the document: a plan by its slug, a section by its number. */
function breadcrumb(page: ConceptPage, facts: ReturnType<typeof useFeatureFacts>): HeadingCrumb[] {
  const parts = parsePageKey(page.key);
  const crumbs: HeadingCrumb[] = page.feature === null ? [] : [featureCrumb(page.feature)];
  if (parts === null || parts.kind === "standalone") {
    return [...crumbs, { label: "Plans" }, { label: parts?.doc ?? page.title, mono: true }];
  }
  if (parts.kind === "summary" && parts.scope === "product" && parts.doc === VISION_DOC) {
    return [...crumbs, { label: page.title }];
  }
  if (parts.kind === "summary" && parts.scope === "metaplan") {
    return [...crumbs, { label: "Metaplan" }];
  }
  const product = parts.scope === "product";
  const overview = parts.kind === "overview";
  const area = product ? "Product spec" : "Plans";
  const areaKey = product ? overviewKey(parts.feature) : metaplanKey(parts.feature);
  const last = overview
    ? { label: page.title }
    : product
      ? { label: sectionLabel(parts.doc, facts.sections) ?? page.title }
      : { label: parts.doc, mono: true };
  return [...crumbs, { label: area, to: pageHref(areaKey) }, last];
}

function implementsChips(page: ConceptPage): HeadingChip[] {
  const view = page.implements;
  if (!view) return [];
  if (view.kind === "not_stated") {
    return [{ key: "implements", label: "Implements", links: [{ label: "Not stated" }] }];
  }
  if (view.links.length === 0) return [];
  return [
    {
      key: "implements",
      label: "Implements",
      links: view.links.map((link) =>
        link.doc && link.number !== null
          ? {
              label: sectionNumberLabel(link.number),
              name: withoutNumber(link.doc.title, link.number),
              to: pageHref(link.doc.page),
            }
          : { label: link.name },
      ),
    },
  ];
}

/**
 * The summary status row, with its Rebuild action at its end and the pending approval's
 * sentence as its detail; the metaplan and the vision draw it as a ledger row instead. A page whose state is null gets no row and so no Rebuild: the server
 * sets state for every expected id (`plan_server/concept/evaluate.rs`), so null occurs only
 * while a cached evaluation predates a new document.
 */
function statusChips(page: ConceptPage, rebuilding: boolean): HeadingChip[] {
  if (summaryInLedger(page)) return [];
  return summaryChips(page, rebuilding).map((chip) => ({
    ...chip,
    action: <SummaryAction page={page} />,
    detail: <SummaryDetail page={page} />,
  }));
}

/** A plan past written keeps its test date as a status beside its state. */
function testedPressure(page: ConceptPage): HeadingChip[] {
  return page.last_pressure === null ? [] : [pressureChip(page.last_pressure, false)];
}

/** A relationship reads as a labelled row of the facts, not in the status card. */
function fact(chip: HeadingChip): HeadingChip {
  return { ...chip, kind: "fact" };
}

/**
 * A written plan's Pressure-test, on the state row beside the settle control: the button's label
 * is the plan's pressure state, so "Not pressure-tested" stands once, and it opens the menu that
 * starts a test. The Codex configuration
 * notice follows it on a line of its own while one blocks.
 */
function PlanPressure(props: { page: ConceptPage; active: boolean }): JSX.Element | null {
  const { page, active } = props;
  if (page.plan_state !== "written" || page.path === null) return null;
  // The record's link goes where the plan's Pressure record card goes, so only a traced plan has it.
  const traced = tracedPlan(page);
  return (
    <>
      <PressureTestAction
        path={page.path}
        date={page.last_pressure}
        active={active}
        recordHref={traced && pressureRecordHref(traced.feature, traced.path)}
      />
      <div className="doc-heading-state__detail plan-fine">
        <PressureTestDetail />
      </div>
    </>
  );
}

/**
 * The status row's chips (the summary's state with its Rebuild, a section's coverage, the open
 * questions, a plan's state), then the facts: what the document implements and the plans that
 * build it. The settle state and a written plan's pressure state are the buttons of the state row,
 * not chips. A feature plan's pressure record has a card of its own under the heading
 * (`PlanExtras`), so only a standalone plan past written keeps its test date as a chip.
 */
function pageChips(
  page: ConceptPage,
  rebuilding: boolean,
  attention: { questions: HeadingChip[]; trace: HeadingChip[] },
) {
  const plan = page.plan_state;
  const { questions, trace } = attention;
  return [
    ...statusChips(page, rebuilding),
    ...trace,
    ...questions,
    ...(plan === null ? [] : [planStateChip(plan)]),
    ...(plan === null || plan === "written" || tracedPlan(page) ? [] : testedPressure(page)),
    ...implementsChips(page).map(fact),
    ...builtByChips(page).map(fact),
  ];
}

/** The page's source: inside its feature when the path belongs to one, else in file mode. */
function sourceAddress(path: string): string {
  return featureSourceHref(featureOfDocPath(path), path);
}

/**
 * Edit and Discuss over the page's source document, read through the store the passages use.
 * Rendered with `key={path}`: `useKeptDoc` keeps the last loaded document across a path change,
 * so a reused instance would act on the previous page's file until the new one loads. Neither
 * action shows when the source failed to load (the file is gone or unreadable).
 */
function SourceActions({ path }: { path: string }): JSX.Element | null {
  const { doc, failed } = useKeptDoc(path);
  const pressureActive = usePressureActive(doc);
  if (failed) return null;
  return <HeadingActions doc={doc} pressureActive={pressureActive} />;
}

/**
 * Open all while any openable sentence is closed, Fold all once none is. One element under both
 * labels, so focus stays on it when the label turns.
 */
export function OpenAllButton({ page }: { page: ConceptPage }): JSX.Element {
  const allOpen = useAllOpen(page);
  const openAll = useOpenAll(page);
  const foldAll = useFoldAll(page);
  const Icon = allOpen ? FoldVertical : UnfoldVertical;
  return (
    <button type="button" {...HEADING_BUTTON} onClick={allOpen ? foldAll : openAll}>
      <Icon aria-hidden="true" />
      {allOpen ? "Fold all" : "Open all"}
    </button>
  );
}

/**
 * The document behind a summary page, for its settle state, scope and plan extras. Overviews are
 * not documents of their own (they summarise the vision or the metaplan), so only a summary page
 * carries them.
 */
function tracedDocument(page: ConceptPage): { feature: string; path: string } | null {
  if (page.feature === null || !page.path || parsePageKey(page.key)?.kind !== "summary") {
    return null;
  }
  return { feature: page.feature, path: page.path };
}

/**
 * The settle control with the hash of the source document: `/api/settle` compares it with the
 * source's hash, not the summary file's (`page.file_hash`). Rendered with `key={path}` for the
 * same reason as `SourceActions`.
 */
function SettleOfSource({ feature, path }: { feature: string; path: string }): JSX.Element {
  const { doc } = useKeptDoc(path);
  const locked = docIsLocked(doc);
  return <SettleControl feature={feature} path={path} hash={doc?.hash ?? null} locked={locked} />;
}

/** The settle control leads the state row: its button's label is the document's settle state. */
function TraceSettle({ page }: { page: ConceptPage }): JSX.Element | null {
  const document = tracedDocument(page);
  if (!document) return null;
  return <SettleOfSource key={document.path} feature={document.feature} path={document.path} />;
}

/** The scope standfirst under the title: the document's stub, with its changed note. */
function TraceScope({ page }: { page: ConceptPage }): JSX.Element | null {
  const document = tracedDocument(page);
  if (!document) return null;
  return <ScopeLine feature={document.feature} path={document.path} />;
}

/**
 * The traced document of a feature plan: a summary page of the engineering scope (`plan_state` is
 * not set on every plan page). A standalone plan has no feature, so it has none.
 */
function tracedPlan(page: ConceptPage): { feature: string; path: string } | null {
  const document = tracedDocument(page);
  const key = parsePageKey(page.key);
  return document && key?.kind === "summary" && key.scope === "engineering" ? document : null;
}

/** A plan's pressure record, what it implements and its briefs, under its heading. */
function TracePlan({ page }: { page: ConceptPage }): JSX.Element | null {
  const plan = tracedPlan(page);
  if (!plan) return null;
  return <PlanExtras feature={plan.feature} path={plan.path} pressure={page.last_pressure} />;
}

/**
 * The title of a section or plan summary page led by its document number (`01`, `13`), which the
 * heading draws in muted ink (`trace-heading.css`) in place of a document mark. Null for any other
 * page, which keeps its plain title and mark.
 */
function numberedTitle(page: ConceptPage): ReactNode {
  const parts = parsePageKey(page.key);
  if (parts?.kind !== "summary") return null;
  let number: string | null = null;
  if (parts.scope === "product" && parts.doc !== VISION_DOC) {
    const digits = /^section_(\d+)$/.exec(parts.doc)?.[1];
    number = digits === undefined ? null : digits.padStart(2, "0");
  } else if (parts.scope === "engineering") {
    number = slugNumber(parts.doc);
  }
  if (number === null) return null;
  // A section's title already opens with its number ("04 — Summary Engine"); the muted number
  // replaces that prefix instead of doubling it.
  const name = withoutNumber(page.title, number);
  return (
    <>
      <span className="doc-heading-number" aria-hidden="true">
        {number}
      </span>{" "}
      <Untrusted text={name} />
    </>
  );
}

/**
 * Summary metadata and navigation adapted to the shared document heading. A page with a source
 * reads that document for its My calls. The heading stays mounted across pages (focus and open
 * popovers survive a navigation), so the kept document of the previous page is ignored until the
 * new one loads.
 */
export function PageHeading({
  page,
  ledger,
}: {
  page: ConceptPage;
  /** Rows the page adds to the heading's ledger, above its status row (the run, a bulk settle). */
  ledger?: ReactNode;
}): JSX.Element {
  if (!page.path) return <PageHeadingOf page={page} calls={[]} ledger={ledger} />;
  return <PageHeadingWithCalls page={page} path={page.path} ledger={ledger} />;
}

function PageHeadingWithCalls(props: {
  page: ConceptPage;
  path: string;
  ledger: ReactNode;
}): JSX.Element {
  const { page, path, ledger } = props;
  const kept = useKeptDoc(path).doc;
  const calls = useMyCallChips(path, kept?.path === repoRel(path) ? kept : null, true);
  return <PageHeadingOf page={page} calls={calls} ledger={ledger} />;
}

function PageHeadingOf(props: {
  page: ConceptPage;
  calls: HeadingChip[];
  ledger: ReactNode;
}): JSX.Element {
  const { page, calls, ledger } = props;
  const facts = useFeatureFacts(page.feature);
  const questions = [...useOpenQuestionChips(page.path, true), ...calls];
  const document = tracedDocument(page);
  const trace = useTraceChips(document?.feature ?? null, document?.path ?? null);
  const rebuilding = isRebuilding(page);
  const run = usePressureRun();
  const pressureActive = page.path !== null && run?.plan === page.path && run.ended === null;
  const notice = page.plan_state === null ? null : LOCKED_NOTICES[page.plan_state];
  const numbered = numberedTitle(page);
  return (
    <>
      <DocHeading
        breadcrumb={breadcrumb(page, facts)}
        kind={numbered === null ? docKindOfPageKey(page.key) : null}
        title={numbered ?? page.title}
        rebuilding={rebuilding}
        path={null}
        changes={null}
        views={{
          current: "summary",
          summaryTo: null,
          sourceTo: page.path ? sourceAddress(page.path) : null,
        }}
        standfirst={<TraceScope page={page} />}
        state={
          <>
            <TraceSettle page={page} />
            <PlanPressure page={page} active={pressureActive} />
          </>
        }
        actions={
          // One group after the view switch: Edit and Discuss over the page's source, then Open
          // all. The group is hidden by CSS when nothing renders in it.
          <ToolbarGroup>
            {page.path && <SourceActions key={page.path} path={page.path} />}
            {hasOpenable(page) && <OpenAllButton page={page} />}
          </ToolbarGroup>
        }
        chips={pageChips(page, rebuilding, { questions, trace })}
      >
        <LedgerRows>
          {summaryInLedger(page) && <PageSummariesRow page={page} />}
          {ledger}
        </LedgerRows>
        {notice && <LockNotice notice={notice} />}
        <PageHeaderExtras page={page} />
      </DocHeading>
      <TracePlan page={page} />
    </>
  );
}
