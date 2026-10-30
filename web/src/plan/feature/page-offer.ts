import type { ChecklistView, OfferKind, OfferView, StepView } from "@/plan/api/schema";
import { paceChoice } from "@/plan/feature/pace";

/** The two overview pages that carry gaps above their text, and the step each one's gaps belong to. */
const GAP_STEP = { product: 2, engineering: 4 } as const;
export type GapScope = keyof typeof GAP_STEP;

/** The feature page a pathname shows: `/f/<feature>/<page>[/<doc>]` gives `[page, doc]`. */
export function featurePage(pathname: string): { page: string; doc: string | null } | null {
  const [root, , page, doc] = pathname.split("/").filter(Boolean);
  return root === "f" && page ? { page, doc: doc ?? null } : null;
}

/** `product` and `engineering` when the pathname is exactly that overview, else null. */
function gapScopeOf(pathname: string): GapScope | null {
  const shown = featurePage(pathname);
  if (shown === null || shown.doc !== null) return null;
  return shown.page === "product" || shown.page === "engineering" ? shown.page : null;
}

export function gapStep(checklist: ChecklistView, scope: GapScope): StepView | undefined {
  return checklist.steps.find((step) => step.number === GAP_STEP[scope]);
}

/** An offer of this kind wherever the checklist carries one: primary, secondary, then item offers. */
function offerOfKind(checklist: ChecklistView, kind: OfferKind): OfferView | undefined {
  return [checklist.primary, ...checklist.secondary, ...checklist.items.map((item) => item.offer)]
    .filter((offer) => offer !== null)
    .find((offer) => offer.kind === kind);
}

/** The offer a missing vision or metaplan page draws for itself. */
export function missingDocumentOffer(
  checklist: ChecklistView,
  page: string,
): OfferView | undefined {
  if (page === "vision" && !checklist.has_product) return offerOfKind(checklist, "start_vision");
  if (page === "metaplan" && checklist.metaplans.length === 0) {
    return offerOfKind(checklist, "start_metaplan");
  }
  return undefined;
}

/** The ids of the offers the pace choice draws on an overview: both cards, or none without a choice. */
export function paceOfferIds(checklist: ChecklistView, scope: GapScope): string[] {
  const step = gapStep(checklist, scope);
  const choice = step === undefined ? null : paceChoice(checklist, step);
  return choice === null ? [] : [choice.guided.id, choice.team.id];
}

/** The slug a route segment names; a malformed escape names the segment itself, matching no plan. */
function slugOf(doc: string): string {
  try {
    return decodeURIComponent(doc);
  } catch {
    return doc;
  }
}

/**
 * Whether the plan page `engineering/<doc>` draws `primary` itself: a written plan has a
 * Pressure-test button for its own slug and a Start a run button (`run/plan-run-actions.tsx`).
 */
function planPageDraws(checklist: ChecklistView, doc: string, primary: OfferView): boolean {
  const slug = slugOf(doc);
  const state = checklist.plan_rows.find((plan) => plan.slug === slug)?.state;
  if (state !== "written") return false;
  return (
    primary.kind === "start_run" || (primary.kind === "pressure_test" && primary.slug === slug)
  );
}

/**
 * Whether the page at `pathname` draws the checklist's primary offer itself, so the spine shows
 * its sentence only. Each offer has one owner, decided by its id: the pace choice on an overview
 * (only when it draws that offer), a missing document's page, or a written plan's page.
 */
export function pageDrawsPrimary(checklist: ChecklistView, pathname: string): boolean {
  const { primary } = checklist;
  if (primary === null) return false;
  const scope = gapScopeOf(pathname);
  if (scope !== null) return paceOfferIds(checklist, scope).includes(primary.id);
  const shown = featurePage(pathname);
  if (shown === null) return false;
  if (shown.page === "engineering" && shown.doc !== null) {
    return planPageDraws(checklist, shown.doc, primary);
  }
  return missingDocumentOffer(checklist, shown.page)?.id === primary.id;
}
