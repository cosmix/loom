import { expect, it } from "vitest";

import { configResponseSchema } from "@/api/config";
import fixtureJson from "@/api/fixtures/config.json";
import { sectionRows } from "@/components/settings-model";

const fixture = configResponseSchema.parse(fixtureJson);

it("the plan section rows carry a caption and pair model with effort", () => {
  const plan = sectionRows(fixture.entries).find((s) => s.section === "plan");
  expect(plan).toBeDefined();
  expect(plan?.caption).toBe("model and effort for loom plan's sessions");

  const rows = plan?.rows ?? [];
  expect(rows).toHaveLength(6);
  const pairs = rows.map((row) =>
    row.kind === "pair" ? [row.model.name, row.effort.name] : [row.kind, row.entry.name],
  );
  expect(pairs).toEqual([
    ["plan.document_summary_model", "plan.document_summary_effort"],
    ["plan.overview_model", "plan.overview_effort"],
    ["single", "plan.summary_backend"],
    ["plan.summary_codex_model", "plan.summary_codex_effort"],
    ["plan.spec_authoring_model", "plan.spec_authoring_effort"],
    ["plan.plan_authoring_model", "plan.plan_authoring_effort"],
  ]);
  expect(rows.find((row) => row.kind === "pair" && row.label === "summary_codex")).toMatchObject({
    caption: "summaries and overviews, Codex",
  });
  expect(
    fixture.entries
      .filter((entry) =>
        [
          "plan.document_summary_effort",
          "plan.overview_model",
          "plan.summary_backend",
          "plan.summary_codex_model",
          "plan.summary_codex_effort",
        ].includes(entry.name),
      )
      .map((entry) => [entry.name, entry.default]),
  ).toEqual([
    ["plan.document_summary_effort", "high"],
    ["plan.overview_model", "sonnet"],
    ["plan.summary_backend", "claude"],
    ["plan.summary_codex_model", "gpt-6.1-terra"],
    ["plan.summary_codex_effort", "high"],
  ]);
});
