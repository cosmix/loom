import { useAtomValue } from "jotai/react";
import { CircleCheck, CircleDashed, MessageCircleQuestion } from "lucide-react";
import type { ReactNode } from "react";
import { Link } from "react-router";

import type { ChecklistView, OfferView } from "@/plan/api/schema";
import type { DraftTeamView } from "@/plan/api/schema-kickoff";
import { Untrusted } from "@/plan/chat/visible-controls";
import { SentenceText } from "@/plan/concept/sentence-text";
import { paceOfferIds, type GapScope } from "@/plan/feature/page-offer";
import { featureHref } from "@/plan/feature/use-feature";
import { slugNumber } from "@/plan/files/heading-parts";
import { planSlug } from "@/plan/files/tree";
import { OfferButton } from "@/plan/offers/offer-button";
import { authorBlockReasonAtom, draftTeamAtom } from "@/plan/state/atoms";
import { StateMark } from "@/plan/ui/activity-glyphs";
import { PLAN_STATE_WORD_GLYPHS, type Glyph, type PlanState } from "@/plan/ui/glyphs";
import { plural } from "@/plan/ui/plural";

export type QueueKind = "section" | "plan";

type QueueRow = {
  key: string;
  number: string;
  name: string;
  /** Section names are Markdown phrasing (backticked code); plan names are slugs, shown verbatim. */
  markdown: boolean;
  /** The concept page of a written document; null while it is missing. */
  to: string | null;
  /** Repository path of the document, to tell which row a teammate is drafting. */
  path: string | null;
  /** The word beside the row's glyph: "Written", "Missing", or a plan's lifecycle state. */
  state: string;
  icon: Glyph;
  /** The `tone-*` class that colours the glyph, for a finished document. */
  tone?: string;
  written: boolean;
  questions: number;
  /** The offer that writes the missing document. */
  offer: OfferView | null;
};

function planIcon(state: string): Glyph {
  return Object.hasOwn(PLAN_STATE_WORD_GLYPHS, state)
    ? PLAN_STATE_WORD_GLYPHS[state as PlanState]
    : CircleCheck;
}

function capitalised(word: string): string {
  return word.charAt(0).toUpperCase() + word.slice(1);
}

function sectionRows(checklist: ChecklistView): QueueRow[] {
  return checklist.sections.map((section) => {
    const path = `doc/product/${checklist.feature}/section_${section.number}.md`;
    const offer = checklist.items.find(
      (item) =>
        item.offer?.kind === "write_section" &&
        (item.path === path || item.offer.number === Number(section.number)),
    )?.offer;
    return {
      key: section.number,
      number: section.number,
      name: section.name,
      markdown: true,
      to: section.written
        ? featureHref(checklist.feature, `product/section_${section.number}`)
        : null,
      path,
      state: section.written ? "Written" : "Missing",
      icon: section.written ? CircleCheck : CircleDashed,
      tone: section.written ? "tone-completed" : undefined,
      written: section.written,
      questions: section.open_questions,
      offer: offer ?? null,
    };
  });
}

/** Plans a metaplan lists: the written ones from `plan_rows`, then the ones still to write. */
function planRows(checklist: ChecklistView): QueueRow[] {
  const written = checklist.plan_rows
    .filter((plan) => plan.listed)
    .map((plan) => ({
      slug: plan.slug,
      to: featureHref(checklist.feature, `engineering/${encodeURIComponent(plan.slug)}`),
      path: plan.path,
      state: capitalised(plan.state),
      icon: planIcon(plan.state),
      tone: plan.state === "done" ? "tone-completed" : undefined,
      written: true,
      offer: null,
    }));
  const missing = checklist.items
    .filter((item) => item.offer?.kind === "write_plan")
    .map((item) => ({
      slug: item.offer?.slug ?? planSlug(item.path?.split("/").at(-1) ?? null) ?? item.text,
      to: null,
      path: item.path,
      state: "Missing",
      icon: CircleDashed,
      tone: undefined,
      written: false,
      offer: item.offer,
    }));
  return [...written, ...missing].map(({ slug, ...row }, index) => ({
    ...row,
    key: `${index}:${slug}`,
    // The number the slug carries, else the row's place: written plans list first, so the place
    // alone would number a plan by when it was written.
    number: slugNumber(slug) ?? String(index + 1).padStart(2, "0"),
    name: slug,
    markdown: false,
    questions: 0,
  }));
}

/** Sections come from the checklist; plans from `plan_rows`, with the missing ones from the offers. */
export function queueRows(checklist: ChecklistView, kind: QueueKind): QueueRow[] {
  return kind === "section" ? sectionRows(checklist) : planRows(checklist);
}

/** Whether any row of the queue still has its document to write. */
export function hasGaps(checklist: ChecklistView, kind: QueueKind): boolean {
  return queueRows(checklist, kind).some((row) => !row.written);
}

function Status({
  icon: Icon,
  tone,
  children,
}: {
  icon: Glyph;
  tone?: string;
  children: ReactNode;
}) {
  return (
    <span className="plan-status plan-fine">
      <Icon className={tone} aria-hidden="true" />
      {children}
    </span>
  );
}

function RowStatus({ row, drafting }: { row: QueueRow; drafting: boolean }) {
  return (
    <>
      {drafting ? (
        <span className="plan-status plan-fine">
          <StateMark kind="drafting" />
          Drafting…
        </span>
      ) : (
        <Status icon={row.icon} tone={row.tone}>
          <Untrusted text={row.state} />
        </Status>
      )}
      {row.questions > 0 && (
        <Status icon={MessageCircleQuestion} tone="tone-warning">
          {plural(row.questions, "open question")}
        </Status>
      )}
    </>
  );
}

/**
 * "Next" stands in for the button of a row whose offer the pace choice already draws, either
 * card's. While authoring is blocked a row draws no button.
 */
function RowAction({ row, drawn }: { row: QueueRow; drawn: string[] }) {
  const blocked = useAtomValue(authorBlockReasonAtom) !== null;
  if (row.offer === null) return null;
  if (drawn.includes(row.offer.id)) return <span className="plan-queue__next">Next</span>;
  if (blocked) return null;
  return <OfferButton offer={row.offer} size="sm" />;
}

function isDrafting(row: QueueRow, team: DraftTeamView | null): boolean {
  return (
    row.path !== null &&
    (team?.teammates.some(
      (teammate) => teammate.state === "working" && teammate.current === row.path,
    ) ??
      false)
  );
}

const QUEUE_SCOPE: Record<QueueKind, GapScope> = { section: "product", plan: "engineering" };

export function Queue({ checklist, kind }: { checklist: ChecklistView; kind: QueueKind }) {
  const team = useAtomValue(draftTeamAtom);
  const rows = queueRows(checklist, kind);
  if (rows.length === 0) return null;
  const drawn = paceOfferIds(checklist, QUEUE_SCOPE[kind]);
  return (
    <ul
      aria-label={kind === "section" ? "Section queue" : "Plan queue"}
      className="plan-list plan-queue"
      data-kind={kind}
    >
      {rows.map((row) => (
        <li key={row.key}>
          <span className="plan-queue__num">
            <Untrusted text={row.number} />
          </span>
          <span className="plan-queue__name">
            {row.to ? (
              <Link to={row.to}>
                {row.markdown ? <SentenceText text={row.name} /> : <Untrusted text={row.name} />}
              </Link>
            ) : row.markdown ? (
              <SentenceText text={row.name} />
            ) : (
              <Untrusted text={row.name} />
            )}
          </span>
          <span className="plan-queue__side">
            <RowStatus row={row} drafting={isDrafting(row, team)} />
            <RowAction row={row} drawn={drawn} />
          </span>
        </li>
      ))}
    </ul>
  );
}
