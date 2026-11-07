import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";

import type { StandalonePlanView } from "@/plan/api/schema";
import { standalonePlanHref } from "@/plan/home/plan-href";
import { StandaloneTable } from "@/plan/home/standalone-table";

function plan(slug: string, hasSummary: boolean): StandalonePlanView {
  return {
    slug,
    path: `doc/plans/PLAN-${slug}.md`,
    title: `Plan ${slug}`,
    state: "written",
    last_change: null,
    pressure_date: null,
    has_summary: hasSummary,
  };
}

describe("standalonePlanHref", () => {
  it("opens the Summary page of a plan that has a summary", () => {
    expect(standalonePlanHref(plan("fix-search", true))).toBe("/plans/fix-search");
  });

  it("opens the source of a plan without a summary", () => {
    expect(standalonePlanHref(plan("fix-search", false))).toBe("/files/plans/PLAN-fix-search.md");
  });

  it("opens the source when the slug cannot name a summary page", () => {
    expect(standalonePlanHref(plan("odd slug", true))).toBe("/files/plans/PLAN-odd%20slug.md");
  });
});

describe("the standalone table rows", () => {
  it("link a summarised plan to its Summary and any other plan to its source", () => {
    render(
      <MemoryRouter>
        <StandaloneTable
          plans={[plan("with", true), plan("without", false)]}
          selected={new Set()}
          onToggle={() => {}}
          onSelect={() => {}}
          actions={null}
        />
      </MemoryRouter>,
    );

    const href = (name: string) => screen.getByRole("link", { name }).getAttribute("href");
    expect(href("Plan with")).toBe("/plans/with");
    expect(href("Plan without")).toBe("/files/plans/PLAN-without.md");
  });
});
