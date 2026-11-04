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
