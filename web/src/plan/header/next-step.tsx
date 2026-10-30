import { cn } from "cn";
import type { ChecklistView, OfferView } from "@/plan/api/schema";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { isCurrentStep, readyLabel } from "@/plan/header/step-tracker";
import { OfferButton } from "@/plan/offers/offer-button";
import { DocMark } from "@/plan/ui/doc-mark";
import { STEP_DOC_KIND } from "@/plan/ui/glyphs";

// DisabledReason owns the primary measures (30px, radius 8px, 13px/600); a long offer label wraps
// instead of overflowing, so the height becomes a minimum.
const ACTION_CLASS =
  "plan-next-action min-w-0 max-w-full [&_button]:h-auto [&_button]:min-h-[30px] [&_button]:py-1 [&_button]:text-left [&_button]:whitespace-normal [&_span]:max-w-full [&_span]:break-words";

// The narrow row gives the button what the rail summary leaves over: one line with an ellipsis,
// and the disabled reason under it clamped the same way (rules in header.css).
const COMPACT_CLASS = "plan-next-action plan-next-action-compact";

interface OfferText {
  title: string;
  detail: string;
}

function planName(offer: OfferView, owner: string): string {
  return (
    offer.slug ??
    offer.target
      ?.split("/")
      .pop()
      ?.replace(/^PLAN-/, "")
      .replace(/\.md$/, "") ??
    owner
  );
}

function untestedDetail(untested: number): string {
  const count = untested === 1 ? "1 plan is" : `${untested} plans are`;
  return `${count} untested. You can also start a run.`;
}

type OfferKind = OfferView["kind"];

/** What every per-kind builder reads: the names an offer's sentence is built from. */
interface OfferContext {
  offer: OfferView;
  checklist: ChecklistView;
  owner: string;
  plan: string;
  section: string;
}

type OfferTextBuilder = (context: OfferContext) => OfferText;

type DocumentKind =
  | "start_vision"
  | "fix_vision_table"
  | "write_section"
  | "add_section_to_vision"
  | "start_metaplan"
  | "refine_spec"
  | "draft_all_sections"
  | "move_into_feature";

/** Offers about the vision and the product spec documents. */
const DOCUMENT_TEXT: Record<DocumentKind, OfferTextBuilder> = {
  start_vision: ({ owner }) => ({
    title: "Write the product vision",
    detail: `The vision for ${owner} is the next document to write.`,
  }),
  fix_vision_table: ({ owner }) => ({
    title: "Fix the section table",
    detail: `The vision for ${owner} needs a readable section table.`,
  }),
  write_section: ({ section }) => ({
    title: "Write the next section",
    detail: `Section ${section} is the next product spec document to write.`,
  }),
  add_section_to_vision: ({ section }) => ({
    title: "Add a section to the vision",
    detail: `Section ${section} needs an entry in the vision's section table.`,
  }),
  start_metaplan: ({ owner }) => ({
    title: "Start the metaplan",
    detail: `The product spec for ${owner} is ready for its first metaplan.`,
  }),
  refine_spec: ({ owner }) => ({
    title: "Refine the product spec",
    detail: `The spec for ${owner} is ready to reflect what was built.`,
  }),
  draft_all_sections: ({ owner }) => ({
    title: "Draft the missing sections",
    detail: `The product spec sections missing for ${owner} are ready to draft.`,
  }),
  move_into_feature: ({ offer, plan }) => ({
    title: "Give the document a feature",
    detail: `${offer.target ?? plan} needs a feature of its own.`,
  }),
};

/** Offers about the engineering plans and the run. */
const PLAN_TEXT: Record<Exclude<OfferKind, DocumentKind>, OfferTextBuilder> = {
  write_plan: ({ plan }) => ({
    title: "Write the next plan",
    detail: `Plan ${plan} is the next plan to write.`,
  }),
  pressure_test: ({ checklist, plan }) => ({
    title: "Pressure-test the next plan",
    detail:
      checklist.untested > 0
        ? untestedDetail(checklist.untested)
        : `Plan ${plan} is ready for pressure testing.`,
  }),
  start_run: ({ owner }) => ({
    title: "Start the run",
    detail: `The plans for ${owner} are ready to run.`,
  }),
  new_metaplan_round: ({ owner }) => ({
    title: "Begin the next metaplan round",
    detail: `The next round for ${owner} is ready to begin.`,
  }),
  draft_all_plans: ({ owner }) => ({
    title: "Draft the missing plans",
    detail: `The plans missing for ${owner} are ready to draft.`,
  }),
};

const OFFER_TEXT: Record<OfferKind, OfferTextBuilder> = { ...DOCUMENT_TEXT, ...PLAN_TEXT };

/** A short imperative title over a one-line detail, per offer kind. */
function offerText(offer: OfferView, checklist: ChecklistView): OfferText {
  const owner = offer.feature ?? checklist.feature;
  const section =
    offer.number === undefined ? (offer.target ?? owner) : String(offer.number).padStart(2, "0");
  return OFFER_TEXT[offer.kind]({ offer, checklist, owner, plan: planName(offer, owner), section });
}

function fallbackTitle(checklist: ChecklistView): string {
  return checklist.items[0]?.text || readyLabel(checklist.current) || "Nothing to do next";
}

/**
 * The next action beside the rail. `compact` renders only the offer button, for the narrow
 * header row where the sentence has no room.
 */
export function NextStepCard({
  checklist,
  compact = false,
}: {
  checklist: ChecklistView | null;
  compact?: boolean;
}) {
  if (checklist === null) return null;

  const { primary } = checklist;
  if (compact) {
    return primary ? (
      // The label stays the button's accessible name; the title keeps it readable once truncated.
      <div className={COMPACT_CLASS} title={visibleControls(primary.label)}>
        <OfferButton offer={primary} />
      </div>
    ) : null;
  }

  const step = checklist.steps.find((candidate) => isCurrentStep(checklist.current, candidate));
  const kind = step ? (STEP_DOC_KIND[step.number - 1] ?? "plan") : "plan";
  const text = primary
    ? offerText(primary, checklist)
    : { title: fallbackTitle(checklist), detail: null };

  return (
    <div className="plan-next-step flex min-w-0 flex-[1_1_280px] items-center gap-3 rounded-[10px] border border-dashed border-border py-2 pr-2.5 pl-3 font-sans text-card-foreground">
      <DocMark kind={kind} size="lg" />
      <div className="min-w-0">
        <p
          className="plan-next-title m-0 text-[13px] leading-[18px] font-semibold text-foreground"
          title={visibleControls(text.title)}
        >
          <Untrusted text={text.title} />
        </p>
        {text.detail !== null && (
          <p
            className="m-0 text-[12.5px] leading-[17px] break-words whitespace-normal text-muted-foreground"
            title={visibleControls(text.detail)}
          >
            <Untrusted text={text.detail} />
          </p>
        )}
      </div>
      {primary && (
        <div className={cn(ACTION_CLASS, "ml-auto")}>
          <OfferButton offer={primary} />
        </div>
      )}
    </div>
  );
}
