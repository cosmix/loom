import { FoldVertical, UnfoldVertical } from "lucide-react";
import type { JSX, ReactNode } from "react";

import {
  VISION_DOC,
  docKindOfPageKey,
  overviewKey,
  pageHref,
  parsePageKey,
} from "@/plan/api/page-key";
import type { ConceptPage } from "@/plan/api/schema-concept";
import { usePressureRun } from "@/plan/chat/pressure/use-pressure";
import { Untrusted } from "@/plan/chat/visible-controls";
import { featureOfDocPath, featureSourceHref } from "@/plan/feature/source-href";
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
  useOpenQuestionChips,
} from "@/plan/files/heading-parts";
import { HeadingActions, usePressureActive } from "@/plan/files/doc-heading-view";
import { LockNotice } from "@/plan/files/lock-notice";
import { useTraceChips } from "@/plan/trace/heading-chips";
import { withoutNumber } from "@/plan/trace/model";
import { PlanExtras } from "@/plan/trace/plan-extras";
import { ScopeLine } from "@/plan/trace/scope-line";
import { docIsLocked, SettleControl } from "@/plan/trace/settle-control";
import { Chip } from "@/plan/ui/doc-heading-parts";
import {
  DocHeading,
  HEADING_BUTTON,
  type HeadingChip,
  type HeadingCrumb,
  ToolbarGroup,
} from "@/plan/ui/doc-heading";

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
  const area = product ? "Product spec" : overview ? "Engineering" : "Plans";
  const last = overview
    ? { label: page.title }
    : product
      ? { label: sectionLabel(parts.doc, facts.sections) ?? page.title }
      : { label: parts.doc, mono: true };
  return [...crumbs, { label: area, to: pageHref(overviewKey(parts.feature, parts.scope)) }, last];
}

function implementsChips(page: ConceptPage): HeadingChip[] {
  const view = page.implements;
  if (!view) return [];
  if (view.kind === "not_stated") {
    return [{ key: "implements", label: "Implements: not stated" }];
  }
  if (view.links.length === 0) return [];
  return [
    {
      key: "implements",
      label: "Implements:",
      links: view.links.map((link) =>
        link.doc && link.number !== null
          ? {
              label: sectionNumberLabel(link.number),
              to: pageHref(link.doc.page),
              title: link.doc.title,
            }
          : { label: link.name },
      ),
    },
  ];
}

/**
 * The summary status row, with its Rebuild action at its end and the pending approval's
 * sentence as its detail. A page whose state is null gets no row and so no Rebuild: the server
 * sets state for every expected id (`plan_server/concept/evaluate.rs`), so null occurs only
 * while a cached evaluation predates a new document.
 */
function statusChips(page: ConceptPage, rebuilding: boolean): HeadingChip[] {
  return summaryChips(page, rebuilding).map((chip) => ({
    ...chip,
    action: <SummaryAction page={page} />,
    detail: <SummaryDetail page={page} />,
  }));
}

/**
 * A standalone plan has no pressure record card, so a plan past written (whose row carries no
 * pressure status of its own) keeps its test date as a status beside its state.
 */
function standalonePressure(page: ConceptPage): HeadingChip[] {
  return page.last_pressure === null ? [] : [pressureChip(page.last_pressure, false)];
}

/** A relationship reads on the facts line, not in the status card. */
function fact(chip: HeadingChip): HeadingChip {
  return { ...chip, kind: "fact" };
}

/**
 * What a written plan's state row goes on with: its pressure status (glyph and words), then
 * Pressure-test, one unit after "Written" so the action stays on the line of the state it acts on.
 */
function PlanPressure(props: { status: HeadingChip | null; action: ReactNode }): JSX.Element {
  return (
    <span className="trace-plan-pressure">
      {props.status && <Chip chip={props.status} />}
      {props.action}
    </span>
  );
}

/**
 * A plan's state row. A written plan's row goes on with its pressure status and Pressure-test, the
 * Codex configuration notice as its detail. A feature plan's test date is drawn once, in the
 * pressure record card under the heading (`PlanExtras`), so its row leaves the date out; "Not
 * pressure-tested" and a running test have no card, so they stay.
 */
function planRow(
  page: ConceptPage,
  plan: NonNullable<ConceptPage["plan_state"]>,
  pressureActive: boolean,
): HeadingChip {
  const chip = planStateChip(plan);
  if (plan !== "written" || page.path === null) return chip;
  const pressure = pressureChip(page.last_pressure, pressureActive, page.path);
  const inCard = tracedPlan(page) !== null && page.last_pressure !== null && !pressureActive;
  return {
    ...chip,
    action: <PlanPressure status={inCard ? null : pressure} action={pressure.action} />,
    detail: pressure.detail,
  };
}

/**
 * The status card's rows, what needs attention first (the summary's state with its Rebuild, the
 * document's settle state and coverage, the open questions, a plan's state with a written plan's
 * Pressure-test), then the facts line: what the document implements and the plans that build it.
 * A plan's pressure record has a card of its own under the heading (`PlanExtras`).
 */
function pageChips(
  page: ConceptPage,
  rebuilding: boolean,
  pressureActive: boolean,
  attention: { questions: HeadingChip[]; trace: HeadingChip[] },
) {
  const plan = page.plan_state;
  const { questions, trace } = attention;
  return [
    ...statusChips(page, rebuilding),
    ...trace,
    ...questions,
    ...(plan === null ? [] : [planRow(page, plan, pressureActive)]),
    ...(plan === null || plan === "written" || tracedPlan(page) ? [] : standalonePressure(page)),
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
    // Beside the settle control the button shows its glyph alone (trace-heading.css), so the
    // tooltip carries the words a sighted reader would otherwise lose.
    <button
      type="button"
      {...HEADING_BUTTON}
      title={allOpen ? "Fold all" : "Open all"}
      onClick={allOpen ? foldAll : openAll}
    >
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

/** The settle control is the last of the toolbar's actions on the document. */
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

/** Summary metadata and navigation adapted to the shared document heading. */
export function PageHeading({ page }: { page: ConceptPage }): JSX.Element {
  const facts = useFeatureFacts(page.feature);
  const questions = useOpenQuestionChips(page.path, true);
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
        actions={
          // One group after the view switch: Edit and Discuss over the page's source, then the
          // settle control, then Open all. The group is hidden by CSS when nothing renders in it.
          <ToolbarGroup>
            {page.path && <SourceActions key={page.path} path={page.path} />}
            <TraceSettle page={page} />
            {hasOpenable(page) && <OpenAllButton page={page} />}
          </ToolbarGroup>
        }
        chips={pageChips(page, rebuilding, pressureActive, { questions, trace })}
      >
        {notice && <LockNotice notice={notice} />}
        <PageHeaderExtras page={page} />
      </DocHeading>
      <TracePlan page={page} />
    </>
  );
}
