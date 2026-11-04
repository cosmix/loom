import { fireEvent, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { expect, it } from "vitest";

import { harness, installStartChatLifecycle } from "@/plan/chat/start-chat-test-kit";
import { checklist, project, standalonePlans } from "@/plan/home/features-contracts-kit";
import { StandaloneShelf } from "@/plan/home/standalone-shelf";
import { projectAtom } from "@/plan/state/atoms";

installStartChatLifecycle();

it("two selected plans move to a feature in one message", async () => {
  const h = harness();
  h.store.set(
    projectAtom,
    project({ standalone: standalonePlans(3), checklists: [checklist("demo")] }),
  );
  const { container } = h.mount(
    <MemoryRouter>
      <StandaloneShelf />
    </MemoryRouter>,
  );

  const summary = container.querySelector("summary");
  expect(summary).not.toBeNull();
  fireEvent.click(summary!);
  fireEvent.click(await screen.findByRole("checkbox", { name: "Select Plan 1" }));
  fireEvent.click(screen.getByRole("checkbox", { name: "Select Plan 2" }));
  fireEvent.click(screen.getByRole("button", { name: /Move to a feature/ }));
  fireEvent.click(await screen.findByRole("radio", { name: "demo" }));
  fireEvent.click(screen.getByRole("button", { name: "Move" }));

  const starts = h.send.mock.calls
    .map(([message]) => message as { command?: string; text?: string })
    .filter((message) => message.command === "start");
  expect(starts).toHaveLength(1);
  const text = starts[0]!.text ?? "";
  expect(text).toContain("doc/plans/PLAN-plan-1.md");
  expect(text).toContain("doc/plans/PLAN-plan-2.md");
  expect(text).not.toContain("doc/plans/PLAN-plan-3.md");
});
