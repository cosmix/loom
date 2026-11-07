import { fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { expect, it } from "vitest";

import { standalonePlans } from "@/plan/home/features-contracts-kit";
import { StandaloneTable } from "@/plan/home/standalone-table";

function table(count: number) {
  return (
    <MemoryRouter>
      <StandaloneTable
        plans={standalonePlans(count)}
        selected={new Set()}
        onToggle={() => {}}
        onSelect={() => {}}
        actions={null}
      />
    </MemoryRouter>
  );
}

it("shows every plan once the list shrinks below the filter threshold", () => {
  const { rerender } = render(table(10));
  fireEvent.change(screen.getByRole("searchbox", { name: "Filter plans" }), {
    target: { value: "plan 3" },
  });
  expect(screen.getAllByRole("link")).toHaveLength(1);

  rerender(table(9));

  expect(screen.queryByRole("searchbox", { name: "Filter plans" })).toBeNull();
  expect(screen.getAllByRole("link")).toHaveLength(9);
});

it("marks a pressure-tested plan with a flask and its date, and draws nothing for an untested one", () => {
  const [tested, untested] = standalonePlans(2);
  const plans = [
    { ...tested, pressure_date: "2026-11-05" },
    { ...untested, pressure_date: null },
  ];
  const { container } = render(
    <MemoryRouter>
      <StandaloneTable
        plans={plans}
        selected={new Set()}
        onToggle={() => {}}
        onSelect={() => {}}
        actions={null}
      />
    </MemoryRouter>,
  );

  const rowOf = (slug: string) =>
    [...container.querySelectorAll("tbody tr")].find((row) => row.textContent?.includes(slug));
  expect(rowOf("plan-1")?.querySelector(".lucide-flask-conical")).not.toBeNull();
  expect(rowOf("plan-1")?.querySelector(".sr-only")?.textContent).toBe(
    "Pressure-tested 2026-11-05",
  );
  expect(rowOf("plan-2")?.querySelector(".lucide-flask-conical")).toBeNull();
  expect(rowOf("plan-2")?.querySelector(".shelf-table__pressure")).toBeNull();
});
