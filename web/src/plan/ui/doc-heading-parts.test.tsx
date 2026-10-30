import { cleanup, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it } from "vitest";

import { DocHeading, type DocHeadingProps } from "./doc-heading";

afterEach(cleanup);

function renderHeading(props: Partial<DocHeadingProps> = {}) {
  return render(<DocHeading breadcrumb={[]} kind="plan" title="Execution handoff" {...props} />, {
    wrapper: MemoryRouter,
  });
}

it("sets the header on the card with a hairline rule and a 10px grid", () => {
  const { container } = renderHeading();

  const header = container.querySelector("header") as HTMLElement;
  for (const name of ["bg-card", "border-b", "border-hairline", "gap-2.5", "grid"]) {
    expect(header.classList.contains(name)).toBe(true);
  }
});

it("renders a crumb icon and sets a plan's slug crumb in mono", () => {
  renderHeading({
    breadcrumb: [
      { label: "Demo feature", to: "/f/demo", icon: <svg data-testid="crumb-icon" /> },
      { label: "Plans" },
      { label: "feat-planner-12", mono: true },
    ],
  });

  const nav = screen.getByRole("navigation", { name: "Breadcrumb" });
  expect(screen.getByTestId("crumb-icon").parentElement?.getAttribute("aria-hidden")).toBe("true");
  expect(within(nav).getByRole("link", { name: "Demo feature" }).getAttribute("href")).toBe(
    "/f/demo",
  );
  expect(within(nav).getByText("feat-planner-12").closest("[aria-current]")?.className).toContain(
    "font-mono",
  );
  expect(within(nav).getByText("Plans").closest("[aria-current]")).toBeNull();
});

it("puts an icon in each segment of the view switch", () => {
  renderHeading({ views: { current: "summary", summaryTo: "/summary", sourceTo: "/source" } });

  const group = screen.getByRole("group", { name: "View" });
  expect(group.querySelector("svg.lucide-sparkles")).not.toBeNull();
  expect(group.querySelector("svg.lucide-file-text")).not.toBeNull();
  expect(group.className).toContain("border-hairline");
});

it("lets the actions wrap under the title, on their own row below 900px", () => {
  renderHeading({ actions: <button type="button">Discuss</button> });

  const actions = screen.getByRole("button", { name: "Discuss" }).parentElement as HTMLElement;
  const row = actions.parentElement as HTMLElement;
  expect(actions.className).toContain("max-[900px]:basis-full");
  expect(row.className).toContain("flex-wrap");
  expect(row.firstElementChild?.className).toContain("flex-[1_1_max-content]");
});

it.each([
  ["accent", "--tone-executing"],
  ["warning", "--tone-warning"],
  ["danger", "--tone-blocked"],
] as const)("washes a %s chip at 18%% and tints only its icon", (tone, variable) => {
  renderHeading({
    chips: [{ key: "t", label: "Chip", tone, icon: <svg data-testid="icon" /> }],
  });

  const chip = screen.getByText("Chip").closest("[data-chip]") as HTMLElement;
  expect(chip.getAttribute("data-tone")).toBe(tone);
  expect(chip.className).toContain(`color-mix(in_oklch,var(${variable})_18%,transparent)`);
  expect(screen.getByTestId("icon").parentElement?.className).toContain(`text-(${variable})`);
  expect(chip.className).toContain("text-foreground");
});

it("leaves neutral and ok chips on the muted fill", () => {
  renderHeading({
    chips: [
      { key: "n", label: "Neutral" },
      { key: "o", label: "Fine", tone: "ok", icon: <svg data-testid="ok-icon" /> },
    ],
  });

  expect(screen.getByText("Neutral").closest("[data-chip]")?.className).toContain("bg-muted");
  expect(screen.getByText("Fine").closest("[data-chip]")?.className).toContain("bg-muted");
  expect(screen.getByTestId("ok-icon").parentElement?.className).toContain(
    "text-(--tone-completed)",
  );
});

it("renders chip links after the label, plain when a target is missing", () => {
  renderHeading({
    chips: [
      {
        key: "implements",
        label: "Implements",
        links: [
          { label: "§05", to: "/product/5", title: "Concept mode" },
          { label: "§99", title: "not found", description: "not found" },
        ],
      },
      {
        key: "built",
        label: "Built by",
        linksSeparator: " · ",
        links: [
          { label: "05", to: "/plans/a" },
          { label: "09", to: "/plans/b" },
        ],
      },
    ],
  });

  const implemented = screen.getByText("Implements", { exact: false }).closest("[data-chip]");
  expect(implemented?.textContent).toBe("Implements §05, §99 (not found)");
  expect(screen.getByRole("link", { name: "§05" }).getAttribute("title")).toBe("Concept mode");
  const missing = screen.getByText("§99", { exact: false });
  expect(missing.closest("a")).toBeNull();
  expect(missing.closest("[title]")?.getAttribute("title")).toBe("not found");
  expect(screen.getByText("Built by", { exact: false }).closest("[data-chip]")?.textContent).toBe(
    "Built by 05 · 09",
  );
  expect(screen.getByRole("link", { name: "09" }).getAttribute("href")).toBe("/plans/b");
});
