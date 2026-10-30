import { useAtomValue } from "jotai/react";
import { CircleCheck, CircleDashed, MessageCircleQuestion } from "lucide-react";
import type { ReactNode } from "react";
import { Link } from "react-router";

import type { ChecklistView, ItemView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
import { SentenceText } from "@/plan/concept/sentence-text";
import { plural } from "@/plan/feature/steps";
import { toAddress } from "@/plan/files/address";
import { planSlug } from "@/plan/files/tree";
import { OfferButton } from "@/plan/offers/offer-button";
import { draftTeamAtom } from "@/plan/state/atoms";
import { ActivityGlyph } from "@/plan/ui/activity-glyphs";

type QueueKind = "section" | "plan";

type QueueRow = {
  key: string;
  number: string;
  name: string;
  path: string | null;
  written: boolean;
  questions: number;
  /** The checklist item this row stands for; its offer renders on the row. */
  item: ItemView | null;
  /** Section names are Markdown phrasing (backticked code); plan names are slugs, shown verbatim. */
  markdown: boolean;
};

/** The step a queue of this kind draws from. */
const QUEUE_STEP: Record<QueueKind, number> = { section: 2, plan: 4 };

/** The checklist text of a written plan (`plan <slug>`); "plan X is not in a metaplan" has more words. */
const WRITTEN_PLAN_TEXT = /^plan \S+$/;

/** A step-4 item the plan queue carries: a plan with an offer (missing) or a written one; not an unlisted warning. */
function isQueuedPlan(item: ItemView): boolean {
  return (
    item.step === QUEUE_STEP.plan && (item.offer !== null || WRITTEN_PLAN_TEXT.test(item.text))
  );
}

/** Sections come from the checklist; the plan queue is the step-4 items, whose offers mark the missing ones. */
export function queueRows(checklist: ChecklistView, kind: QueueKind): QueueRow[] {
  if (kind === "section") {
    return checklist.sections.map((section) => ({
      key: section.number,
      number: section.number,
      name: section.name,
      path: `doc/product/${checklist.feature}/section_${section.number}.md`,
      written: section.written,
      questions: section.open_questions,
      markdown: true,
      item:
        checklist.items.find(
          (item) =>
            item.step === QUEUE_STEP.section &&
            item.path === `doc/product/${checklist.feature}/section_${section.number}.md`,
        ) ?? null,
    }));
  }
  return checklist.items.filter(isQueuedPlan).map((item, index) => ({
    key: `${index}:${item.text}`,
    number: String(index + 1).padStart(2, "0"),
    name: planSlug(item.path?.split("/").at(-1) ?? null) ?? item.text,
    path: item.path,
    written: item.offer === null,
    questions: 0,
    item,
    markdown: false,
  }));
}

/** The step's items that no queue row carries; steps without a queue keep all of theirs. */
export function remainingItems(checklist: ChecklistView, step: number): ItemView[] {
  const items = checklist.items.filter((item) => item.step === step);
  const kind = (Object.keys(QUEUE_STEP) as QueueKind[]).find((key) => QUEUE_STEP[key] === step);
  if (kind === undefined) return items;
  const queued = queueRows(checklist, kind).map((row) => row.item);
  return items.filter((item) => !queued.includes(item));
}

function State({ tone, icon, children }: { tone: string; icon: ReactNode; children: ReactNode }) {
  return (
    <span className="plan-queue__state" data-tone={tone}>
      {icon}
      {children}
    </span>
  );
}

function RowName({ row }: { row: QueueRow }) {
  return row.markdown ? <SentenceText text={row.name} /> : <Untrusted text={row.name} />;
}

function RowStates({ row, drafting }: { row: QueueRow; drafting: boolean }) {
  const icon = "size-3.5 shrink-0";
  return (
    <span className="plan-queue__states">
      {drafting ? (
        <State tone="run" icon={<ActivityGlyph kind="drafting" />}>
          Drafting…
        </State>
      ) : row.written ? (
        <State tone="ok" icon={<CircleCheck className={icon} aria-hidden="true" />}>
          Written
        </State>
      ) : (
        <State tone="muted" icon={<CircleDashed className={icon} aria-hidden="true" />}>
          Missing
        </State>
      )}
      {row.questions > 0 && (
        <State tone="warn" icon={<MessageCircleQuestion className={icon} aria-hidden="true" />}>
          {plural(row.questions, "open question")}
        </State>
      )}
    </span>
  );
}

export function Queue({ checklist, kind }: { checklist: ChecklistView; kind: QueueKind }) {
  const team = useAtomValue(draftTeamAtom);
  const rows = queueRows(checklist, kind);
  if (rows.length === 0) return null;
  return (
    <ul
      aria-label={kind === "section" ? "Section queue" : "Plan queue"}
      className="plan-queue"
      data-kind={kind}
    >
      {rows.map((row) => {
        const drafting =
          row.path !== null &&
          (team?.teammates.some(
            (teammate) => teammate.state === "working" && teammate.current === row.path,
          ) ??
            false);
        return (
          <li key={row.key}>
            <span className="plan-queue__num">
              <Untrusted text={row.number} />
            </span>
            <span className="plan-queue__name">
              {row.written && row.path ? (
                <Link to={toAddress(row.path)}>
                  <RowName row={row} />
                </Link>
              ) : (
                <RowName row={row} />
              )}
            </span>
            <span className="plan-queue__side">
              <RowStates row={row} drafting={drafting} />
              {row.item?.offer && <OfferButton offer={row.item.offer} />}
            </span>
          </li>
        );
      })}
    </ul>
  );
}
