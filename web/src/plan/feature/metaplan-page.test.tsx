import { cleanup, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import { checklistViewSchema } from "@/plan/api/schema";
import { clone, makeStubFetch, renderAt } from "@/plan/concept/concept-route-support";
import { projectAtom } from "@/plan/state/atoms";

const fetched: string[] = [];
const conceptKeys = () =>
  fetched
    .filter((raw) => raw.includes("/api/concept/"))
    .map((raw) => decodeURIComponent(raw.replace(/^.*\/api\/concept\//, "")));

beforeEach(() => {
  fetched.length = 0;
  const pages = {
    "atlas/metaplan/index": {
      ...clone(summaryFixture),
      key: "atlas/metaplan/index",
      feature: "atlas",
      path: "doc/plans/atlas/index.md",
    },
  };
  vi.stubGlobal(
    "fetch",
    vi.fn(makeStubFetch(() => ({ pages, docs: {}, failing: false }), fetched)),
  );
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function renderMetaplan(path: string, file: string) {
  const checklist = checklistViewSchema.parse({
    ...checklistFixture.checklists[0]!,
    feature: "atlas",
    metaplans: [{ path: `doc/plans/atlas/${file}`, title: "Roadmap" }],
  });
  return renderAt(path, (store) =>
    store.set(projectAtom, { checklists: [checklist], standalone: [], moves: [] }),
  );
}

it("a metaplan whose file name is no summary stem opens as its source, not as a missing page", async () => {
  renderMetaplan("/f/atlas/metaplan/metaplan_02.md", "metaplan_02.md");
  await waitFor(() => expect(document.querySelector(".plan-document")).not.toBeNull());
  expect(conceptKeys().filter((key) => key.includes("metaplan"))).toEqual([]);
});

it("the bare metaplan route of a metaplan with an invalid stem does not redirect to a summary", async () => {
  const { router } = renderMetaplan("/f/atlas/metaplan", "metaplan_02.md");
  await waitFor(() => expect(document.querySelector(".plan-document")).not.toBeNull());
  expect(router.state.location.pathname).toBe("/f/atlas/metaplan");
  expect(conceptKeys().filter((key) => key.includes("metaplan"))).toEqual([]);
});

it("a metaplan with a valid stem still opens its summary page", async () => {
  renderMetaplan("/f/atlas/metaplan", "index.md");
  await waitFor(() => expect(conceptKeys()).toContain("atlas/metaplan/index"));
  expect(document.querySelector(".plan-document")).toBeNull();
});
