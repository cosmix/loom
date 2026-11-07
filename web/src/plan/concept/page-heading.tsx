import { FoldVertical, UnfoldVertical } from "lucide-react";
import type { JSX } from "react";

import {
  VISION_DOC,
  docKindOfPageKey,
  overviewKey,
  pageHref,
  parsePageKey,
} from "@/plan/api/page-key";
import type { ConceptPage } from "@/plan/api/schema-concept";
import { featureOfDocPath, featureSourceHref } from "@/plan/feature/source-href";
import {
  builtByChips,
  featureCrumb,
  isRebuilding,
  planStateChip,
  pressureChip,
  sectionLabel,
  sectionNumberLabel,
  summaryChips,
  useFeatureFacts,
  useOpenQuestionChips,
} from "@/plan/files/heading-parts";
import { HeadingActions, usePressureActive } from "@/plan/files/doc-heading-view";
import { LockNotice } from "@/plan/files/lock-notice";
import { BriefsCard } from "@/plan/trace/briefs-card";
import { docIsLocked, SettleControl } from "@/plan/trace/settle-control";
import { StubPanel } from "@/plan/trace/stub-panel";
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

/** A relationship reads on the facts line, not in the status card. */
function fact(chip: HeadingChip): HeadingChip {
  return { ...chip, kind: "fact" };
}

/**
 * The status card's rows, what needs attention first (the summary's state with its Rebuild, the
 * open questions, a plan's state and pressure record), then the facts line: what the document
 * implements and the plans that build it.
 */
function pageChips(page: ConceptPage, rebuilding: boolean, questions: HeadingChip[]) {
  const plan = page.plan_state;
  const showPressure = plan !== null || page.last_pressure !== null;
  return [
    ...statusChips(page, rebuilding),
    ...questions,
    ...(plan === null ? [] : [planStateChip(plan)]),
    ...(showPressure
      ? [pressureChip(page.last_pressure, false, plan === "written" ? page.path : null)]
      : []),
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
 * The settle state and stub of a document's summary page. Overviews are not documents of their
 * own (they summarise the vision or the metaplan), so only a summary page carries them.
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

/** The settle control is the toolbar's second group, after the Summary / Source switch. */
function TraceSettle({ page }: { page: ConceptPage }): JSX.Element | null {
  const document = tracedDocument(page);
  if (!document) return null;
  return <SettleOfSource key={document.path} feature={document.feature} path={document.path} />;
}

/** The document's stub, then a plan's briefs in a card of their own. */
function TraceExtras({ page }: { page: ConceptPage }): JSX.Element | null {
  const document = tracedDocument(page);
  if (!document) return null;
  return (
    <>
      <StubPanel feature={document.feature} path={document.path} />
      <BriefsCard feature={document.feature} path={document.path} />
    </>
  );
}

/** Summary metadata and navigation adapted to the shared document heading. */
export function PageHeading({ page }: { page: ConceptPage }): JSX.Element {
  const facts = useFeatureFacts(page.feature);
  const questions = useOpenQuestionChips(page.path, true);
  const rebuilding = isRebuilding(page);
  const notice = page.plan_state === null ? null : LOCKED_NOTICES[page.plan_state];
  return (
    <DocHeading
      breadcrumb={breadcrumb(page, facts)}
      kind={docKindOfPageKey(page.key)}
      title={page.title}
      rebuilding={rebuilding}
      path={null}
      changes={null}
      views={{
        current: "summary",
        summaryTo: null,
        sourceTo: page.path ? sourceAddress(page.path) : null,
      }}
      actions={
        // Two groups of the toolbar bar after the view switch: the settle state with its verb,
        // then the actions over the text. Either group is hidden by CSS when nothing renders
        // inside it.
        <>
          <ToolbarGroup>
            <TraceSettle page={page} />
          </ToolbarGroup>
          <ToolbarGroup>
            {page.path && <SourceActions key={page.path} path={page.path} />}
            {hasOpenable(page) && <OpenAllButton page={page} />}
          </ToolbarGroup>
        </>
      }
      chips={pageChips(page, rebuilding, questions)}
    >
      {notice && <LockNotice notice={notice} />}
      <TraceExtras page={page} />
      <PageHeaderExtras page={page} />
    </DocHeading>
  );
}
