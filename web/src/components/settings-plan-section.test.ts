import { expect, it } from "vitest";

import { configResponseSchema } from "@/api/config";
import fixtureJson from "@/api/fixtures/config.json";
import { sectionRows } from "@/components/settings-model";

const fixture = configResponseSchema.parse(fixtureJson);

it("the plan section rows carry a caption and pair model with effort", () => {
  const plan = sectionRows(fixture.entries).find((s) => s.section === "plan");
  expect(plan).toBeDefined();
  expect(plan?.caption).toBe("what loom plan's sessions run on");

  const rows = plan?.rows ?? [];
  expect(rows).toHaveLength(4);
  const pairs = rows.map((row) =>
    row.kind === "pair" ? [row.model.name, row.effort.name] : [row.kind],
  );
  expect(pairs).toEqual([
    ["plan.document_summary_model", "plan.document_summary_effort"],
    ["plan.overview_model", "plan.overview_effort"],
    ["plan.spec_authoring_model", "plan.spec_authoring_effort"],
    ["plan.plan_authoring_model", "plan.plan_authoring_effort"],
  ]);
});
