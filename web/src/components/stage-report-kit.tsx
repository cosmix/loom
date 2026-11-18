import { render } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type Attention, type Snapshot, type StageSummary } from "@/api/schema";
import { StageModal } from "@/components/stage-modal";
import { TooltipProvider } from "@/components/ui/tooltip";
import { applySnapshot } from "@/state/apply";

export const fixture = snapshotSchema.parse(fixtureJson);

export function fixtureStage(id: string, overrides: Partial<StageSummary> = {}): StageSummary {
  const stage = fixture.status.stages.find((candidate) => candidate.id === id);
  if (!stage) throw new Error(`fixture stage ${id} is missing`);
  return { ...stage, ...overrides };
}

export function renderModal(stage: StageSummary, attention: Attention[], hash = "") {
  const snapshot: Snapshot = {
    ...structuredClone(fixture),
    attention,
    status: {
      ...fixture.status,
      stages: [stage, ...fixture.status.stages.filter((candidate) => candidate.id !== stage.id)],
    },
  };
  const store = createStore();
  applySnapshot(store, snapshot);
  const router = createMemoryRouter([{ path: "/", element: <StageModal /> }], {
    initialEntries: [`/?stage=${stage.id}${hash}`],
  });
  render(
    <Provider store={store}>
      <TooltipProvider>
        <RouterProvider router={router} />
      </TooltipProvider>
    </Provider>,
  );
}
