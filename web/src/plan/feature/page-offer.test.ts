import { expect, it } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema, type OfferKind, type OfferView } from "@/plan/api/schema";

import { missingDocumentOffer, offerOfKind, paceOfferIds } from "./page-offer";

const base = checklistViewSchema.parse({
  ...checklistFixture.checklists[0],
  primary: null,
  secondary: [],
  items: [],
});

function offer(id: string, kind: OfferKind): OfferView {
  return { id, kind, label: id, target: null };
}

it("offerOfKind finds the primary offer of that kind", () => {
  const view = { ...base, primary: offer("p", "start_vision") };
  expect(offerOfKind(view, "start_vision")?.id).toBe("p");
});

it("offerOfKind finds a secondary offer and nothing of a kind that is absent", () => {
  const view = { ...base, secondary: [offer("s", "start_metaplan")] };
  expect(offerOfKind(view, "start_metaplan")?.id).toBe("s");
  expect(offerOfKind(view, "start_vision")).toBeUndefined();
});

it("offerOfKind takes the primary, then the secondary, then the item offers", () => {
  const items = [{ step: 1, text: "i", path: null, offer: offer("item", "start_vision") }];
  const view = {
    ...base,
    primary: offer("p", "start_vision"),
    secondary: [offer("s", "start_vision")],
    items,
  };
  expect(offerOfKind(view, "start_vision")?.id).toBe("p");
  expect(offerOfKind({ ...view, primary: null }, "start_vision")?.id).toBe("s");
  expect(offerOfKind({ ...view, primary: null, secondary: [] }, "start_vision")?.id).toBe("item");
});

it("missingDocumentOffer is the start offer only while the document is missing", () => {
  const start = offer("v", "start_vision");
  const view = { ...base, has_product: false, secondary: [start] };
  expect(missingDocumentOffer(view, "vision")?.id).toBe("v");
  expect(missingDocumentOffer({ ...view, has_product: true }, "vision")).toBeUndefined();
  expect(missingDocumentOffer(view, "plans")).toBeUndefined();
});

it("paceOfferIds is empty when the checklist draws no pace choice", () => {
  expect(paceOfferIds({ ...base, steps: [] }, "product")).toEqual([]);
});
