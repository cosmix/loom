import { render } from "@testing-library/react";
import { MemoryRouter } from "react-router";

import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import { featureOfKey } from "@/plan/api/page-key";
import { conceptPageSchema, type ConceptPage } from "@/plan/api/schema-concept";

import { PageHeading } from "./page-heading";

export function pageWith(overrides: Partial<ConceptPage> = {}): ConceptPage {
  return conceptPageSchema.parse({
    ...structuredClone(summaryFixture),
    state: null,
    implemented_by: [],
    feature: featureOfKey(overrides.key ?? summaryFixture.key),
    ...overrides,
  });
}

export function show(overrides: Partial<ConceptPage> = {}) {
  const page = pageWith(overrides);
  return render(<PageHeading page={page} />, { wrapper: MemoryRouter });
}
