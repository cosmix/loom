import type { ChecklistView, CurrentView, OfferKind, OfferView } from "@/plan/api/schema";
import { plural } from "@/plan/ui/plural";

/** What every per-kind sentence is built from: the names an offer's sentence mentions. */
interface Names {
  offer: OfferView;
  checklist: ChecklistView;
  /** The feature's slug. */
  owner: string;
  plan: string;
  section: string;
}

/** Write and pressure offers always carry the plan's slug (`OfferView.slug`). */
function planName(offer: OfferView, owner: string): string {
  return offer.slug ?? owner;
}

function untestedSentence(untested: number): string {
  const count = untested === 1 ? "1 plan is" : `${plural(untested, "plan")} are`;
  return `${count} untested. You can also start a run.`;
}

/** One sentence per offer kind: the button beside it carries only a short label. */
const SENTENCES: Record<OfferKind, (names: Names) => string> = {
  start_vision: ({ owner }) => `The vision for ${owner} is the next document to write.`,
  fix_vision_table: ({ owner }) => `The vision for ${owner} needs a readable section table.`,
  write_section: ({ section }) => `Section ${section} is the next product spec document to write.`,
  add_section_to_vision: ({ section }) =>
    `Section ${section} needs an entry in the vision's section table.`,
  start_metaplan: ({ owner }) => `The product spec for ${owner} is ready for its first metaplan.`,
  draft_all_sections: ({ owner }) =>
    `The product spec sections missing for ${owner} are ready to draft.`,
  move_into_feature: ({ offer, plan }) => `${offer.target ?? plan} needs a feature of its own.`,
  write_plan: ({ plan }) => `Plan ${plan} is the next plan to write.`,
  pressure_test: ({ checklist, plan }) =>
    checklist.untested > 0
      ? untestedSentence(checklist.untested)
      : `Plan ${plan} is ready for pressure testing.`,
  start_run: ({ owner }) => `The plans for ${owner} are ready to run.`,
  plan_follow_ups: ({ owner }) => `Every plan for ${owner} is done; follow-ups start from here.`,
  review_feature: () => "Review what was built and record what you find.",
  draft_all_plans: ({ owner }) => `The plans missing for ${owner} are ready to draft.`,
};

/** "Ready to run" / "All plans done" when no step is current, else null. */
function readyLabel(current: CurrentView): string | null {
  if (current.kind === "ready") return "Ready to run";
  if (current.kind === "ready_all_done") return "All plans done";
  return null;
}

/** The sentence for the checklist's next step; the feature is named by its slug. */
export function nextSentence(checklist: ChecklistView): string {
  const { primary } = checklist;
  if (primary === null) {
    return checklist.items[0]?.text || readyLabel(checklist.current) || "Nothing to do next";
  }
  const owner = primary.feature ?? checklist.feature;
  const section =
    primary.number === undefined
      ? (primary.target ?? owner)
      : String(primary.number).padStart(2, "0");
  return SENTENCES[primary.kind]({
    offer: primary,
    checklist,
    owner,
    plan: planName(primary, owner),
    section,
  });
}
