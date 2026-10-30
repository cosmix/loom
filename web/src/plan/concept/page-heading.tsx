import { UnfoldVertical } from "lucide-react";
import type { JSX } from "react";

import { Button } from "@/components/ui/button";
import { docKindOfPageKey, overviewKey, pageHref, parsePageKey } from "@/plan/api/page-key";
import type { ConceptPage } from "@/plan/api/schema-concept";
import { toAddress } from "@/plan/files/address";
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
import { DocHeading, IBTN, type HeadingChip, type HeadingCrumb } from "@/plan/ui/doc-heading";

import { useOpenAll } from "./open-all";
import { useKeptDoc } from "./passage-doc";
import { PageHeaderExtras } from "./page-header-extras";

const RUN_NOTICE =
  "This plan is running. `loom run` works from the committed copy, so the app keeps it read-only.";
const DONE_NOTICE = "This plan is done. Record changes in a new plan.";
const LOCKED_NOTICES: Partial<Record<NonNullable<ConceptPage["plan_state"]>, string>> = {
  running: RUN_NOTICE,
  done: DONE_NOTICE,
  retired: DONE_NOTICE,
};

/** Feature title, then the area, then the document: a plan by its slug, a section by its number. */
function breadcrumb(page: ConceptPage, facts: ReturnType<typeof useFeatureFacts>): HeadingCrumb[] {
  const parts = parsePageKey(page.key);
  const crumbs: HeadingCrumb[] =
    page.feature === null || facts.title === null ? [] : [featureCrumb(page.feature, facts.title)];
  if (parts === null || parts.kind === "standalone") {
    return [...crumbs, { label: "Plans" }, { label: parts?.doc ?? page.title, mono: true }];
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

function pageChips(page: ConceptPage, rebuilding: boolean, questions: HeadingChip[]) {
  const plan = page.plan_state;
  const showPressure = plan !== null || page.last_pressure !== null;
  return [
    ...(plan === null ? [] : [planStateChip(plan)]),
    ...(showPressure ? [pressureChip(page.last_pressure, false)] : []),
    ...implementsChips(page),
    ...summaryChips(page, rebuilding),
    ...questions,
    ...builtByChips(page),
  ];
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

function OpenAllButton(): JSX.Element {
  const openAll = useOpenAll();
  return (
    <Button type="button" variant="outline" size="sm" className={IBTN} onClick={openAll}>
      <UnfoldVertical aria-hidden="true" />
      Open all
    </Button>
  );
}

/** Summary metadata and navigation adapted to the shared document heading. */
export function PageHeading({ page }: { page: ConceptPage }): JSX.Element {
  const facts = useFeatureFacts(page.feature);
  const questions = useOpenQuestionChips(page.path);
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
        sourceTo: page.path ? toAddress(page.path) : null,
      }}
      actions={
        <>
          {page.path && <SourceActions key={page.path} path={page.path} />}
          {!page.empty && <OpenAllButton />}
        </>
      }
      chips={pageChips(page, rebuilding, questions)}
    >
      {notice && <LockNotice notice={notice} />}
      <PageHeaderExtras page={page} />
    </DocHeading>
  );
}
