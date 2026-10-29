import type { JSX } from "react";

import type { ConceptPage } from "@/plan/api/schema-concept";
import { toAddress } from "@/plan/files/address";
import { DocHeading, type HeadingChip, type HeadingCrumb } from "@/plan/ui/doc-heading";
import { DOC_KIND_COLOUR } from "@/plan/ui/doc-mark";
import {
  OTHER_GLYPHS,
  PLAN_STATE_GLYPHS,
  SUMMARY_STATE_GLYPHS,
  type DocKind,
} from "@/plan/ui/glyphs";

import { PageHeaderExtras } from "./page-header-extras";
import { planStateText, summaryStatus } from "./summary-text";

const LOCKED_NOTICES: Partial<Record<NonNullable<ConceptPage["plan_state"]>, string>> = {
  running: "This plan is running. `loom run` works from the committed copy.",
  done: "This plan is done. Record changes in a new plan.",
  retired: "This plan is done. Record changes in a new plan.",
};

function breadcrumb(page: ConceptPage): HeadingCrumb[] {
  const crumbs: HeadingCrumb[] = [{ label: "Planner" }];
  const scope = page.key.split("/")[0];
  if (scope === "product") crumbs.push({ label: "Product", to: "/product" });
  if (scope === "engineering") crumbs.push({ label: "Plans", to: "/engineering" });
  crumbs.push({ label: page.title });
  return crumbs;
}

function implementsChips(page: ConceptPage): HeadingChip[] {
  const view = page.implements;
  if (!view) return [];
  if (view.kind === "not_stated") {
    return [{ key: "implements", label: "Implements: not stated" }];
  }
  return [
    { key: "implements", label: "Implements:" },
    ...view.links.map((link) => ({
      key: `implements:${link.name}`,
      label: link.doc?.title ?? link.name,
      ...(link.doc ? { to: `/${link.doc.page}` } : {}),
    })),
  ];
}

function implementedByChips(page: ConceptPage): HeadingChip[] {
  if (page.implemented_by.length === 0) return [];
  return [
    { key: "implemented-by", label: "Implemented by:" },
    ...page.implemented_by.map((plan) => ({
      key: `implemented-by:${plan.slug}`,
      label: `${plan.title} (${planStateText(plan.state)})`,
      title: plan.title,
      to: `/engineering/${encodeURIComponent(plan.slug)}`,
    })),
  ];
}

function headingChips(page: ConceptPage, rebuilding: boolean): HeadingChip[] {
  const chips: HeadingChip[] = [];
  const status = summaryStatus(page.state);
  if (status) {
    const summaryState = rebuilding
      ? "rebuilding"
      : page.state?.state === "current" && !page.state.out_of_date
        ? "current"
        : "stale";
    const Glyph = SUMMARY_STATE_GLYPHS[summaryState];
    chips.push({
      key: "summary-status",
      label: status,
      icon: <Glyph />,
      tone: page.state?.out_of_date || page.state?.state === "stale" ? "warning" : "neutral",
    });
  }
  if (page.plan_state !== null) {
    const Glyph = PLAN_STATE_GLYPHS[page.plan_state];
    chips.push({
      key: "plan-state",
      label: planStateText(page.plan_state),
      title: `Plan state: ${planStateText(page.plan_state)}`,
      icon: <Glyph />,
    });
  }
  if (page.last_pressure) {
    const Glyph = OTHER_GLYPHS.pressureTested;
    chips.push({
      key: "last-pressure",
      label: `Last pressure test ${page.last_pressure}`,
      icon: <Glyph />,
    });
  }
  return [...chips, ...implementsChips(page), ...implementedByChips(page)];
}

/** Summary metadata and navigation adapted to the shared document heading. */
export function PageHeading({ page }: { page: ConceptPage }): JSX.Element {
  const kind =
    (Object.keys(DOC_KIND_COLOUR) as DocKind[]).find(
      (candidate) => DOC_KIND_COLOUR[candidate] === page.colour,
    ) ?? null;
  const rebuilding =
    page.state?.state === "rebuilding" || page.state?.out_of_date?.reason === "rebuilding";
  return (
    <DocHeading
      breadcrumb={breadcrumb(page)}
      kind={kind}
      title={page.title}
      rebuilding={rebuilding}
      path={page.path}
      changes={null}
      views={{
        current: "summary",
        summaryTo: null,
        sourceTo: page.path ? toAddress(page.path) : null,
      }}
      chips={headingChips(page, rebuilding)}
      lock={page.plan_state === null ? null : (LOCKED_NOTICES[page.plan_state] ?? null)}
    >
      <PageHeaderExtras page={page} />
    </DocHeading>
  );
}
