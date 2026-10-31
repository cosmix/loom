import hello from "@/plan/api/fixtures/hello.json";
import { checklistViewSchema, type ChecklistView } from "@/plan/api/schema";

/** Fixtures for the context tests: a feature `atlas` with sections 05 and 07 written, 06 not. */
export const section = (number: string, written = true) => ({
  number,
  name: `Section ${number}`,
  written,
  open_questions: 0,
});

const planRow = (slug: string) => ({
  slug,
  path: `doc/plans/atlas/IN_PROGRESS-PLAN-${slug}.md`,
  title: slug,
  state: "written",
  pressure_date: null,
  listed: true,
});

export function checklist(fields: Record<string, unknown> = {}): ChecklistView {
  return checklistViewSchema.parse({
    ...hello.checklists[0],
    feature: "atlas",
    has_product: true,
    sections: [section("05"), section("06", false), section("07")],
    metaplans: [
      { path: "doc/plans/atlas/index.md", title: "Atlas roadmap" },
      { path: "doc/plans/atlas/second-round.md", title: "Second round" },
    ],
    plan_rows: [planRow("atlas-01-documents")],
    ...fields,
  });
}
