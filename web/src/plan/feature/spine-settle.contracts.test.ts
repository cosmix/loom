import { expect, it } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import traceFixture from "@/plan/api/fixtures/trace.json";
import { checklistViewSchema, type ChecklistView } from "@/plan/api/schema";
import { traceViewSchema, type SettleState, type TraceView } from "@/plan/api/schema-trace";

import { spineGroups } from "./spine-model";
import { STEP } from "./steps";

/** Contract: the Product spec head reads amber while a written section is not settled. */

const SECTIONS = ["01", "02"] as const;

/** The fixture feature with every section written and the server's product step marked done. */
function checklist(): ChecklistView {
  const base = checklistFixture.checklists[0]!;
  return checklistViewSchema.parse({
    ...base,
    sections: SECTIONS.map((number) => ({
      number,
      name: `Section ${number}`,
      written: true,
      open_questions: 0,
    })),
    steps: base.steps.map((step) =>
      step.number === STEP.product ? { ...step, mark: "done", count: "2/2" } : step,
    ),
  });
}

/** The fixture trace whose section documents are the two written, settleable sections. */
function trace(settle: Record<(typeof SECTIONS)[number], SettleState>): TraceView {
  const sections = SECTIONS.map((number) => {
    const doc = traceFixture.documents.find((candidate) => candidate.role === "section")!;
    return {
      ...doc,
      path: `doc/product/demo/section_${number}.md`,
      number: Number(number),
      progress: "planned",
      settleable: true,
      settle: settle[number],
    };
  });
  const others = traceFixture.documents.filter((candidate) => candidate.role !== "section");
  return traceViewSchema.parse({ ...traceFixture, documents: [...sections, ...others] });
}

function product(view: TraceView) {
  const group = spineGroups(checklist(), view).find((candidate) => candidate.id === "product");
  expect(group).toBeDefined();
  return group!;
}

it("the product spec head is amber while a written section is unsettled", () => {
  const revised = product(trace({ "01": "settled", "02": "revised" }));
  expect(revised.mark).toBe("done");
  expect(revised.glyph).toEqual({ kind: "attention", word: "Not all settled" });

  const settled = product(trace({ "01": "settled", "02": "settled" }));
  expect(settled.mark).toBe("done");
  expect(settled.glyph).toBeNull();
});
