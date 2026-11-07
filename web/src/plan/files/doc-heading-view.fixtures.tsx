import { cleanup } from "@testing-library/react";
import { afterEach, beforeEach, vi } from "vitest";

import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import { conceptPageSchema } from "@/plan/api/schema-concept";
import type { DocView } from "@/plan/api/schema-files";
import { conceptPageAtom } from "@/plan/concept/page-state";

import { authorStore } from "./doc-heading-kit";
import { summaryPageKey } from "./section-chips";

/** Loads the summary page behind `doc`'s Summary view into `store`, in the given state. */
export function seedSummary(
  store: ReturnType<typeof authorStore>,
  doc: DocView,
  state: "current" | "missing",
) {
  const key = summaryPageKey(doc)!;
  const page = conceptPageSchema.parse({
    ...structuredClone(summaryFixture),
    key,
    state: { state, out_of_date: null },
  });
  store.set(conceptPageAtom(key), { page, previous: null, error: null });
}

/** Registers the per-test clock and cleanup hooks; call once at the top level of a test file. */
export function registerDocHeadingHooks() {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date(2026, 9, 29, 0, 15));
    window.localStorage.clear();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });
}
