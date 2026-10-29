import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { useState } from "react";
import { MemoryRouter } from "react-router";

import { checklistViewSchema, serverInfoSchema } from "@/plan/api/schema";
import { PlanHeader } from "@/plan/header/header";
import { checklistAtom, serverInfoAtom } from "@/plan/state/atoms";

const SAMPLE_CHECKLIST = checklistViewSchema.parse({
  current: { kind: "step", number: 3 },
  steps: [
    { number: 1, name: "Vision", mark: "done" },
    { number: 2, name: "Product spec", mark: "gap" },
    { number: 3, name: "Metaplan", mark: "active" },
    { number: 4, name: "Plans", mark: "optional" },
    { number: 5, name: "Plan review", mark: "optional" },
    { number: 6, name: "Run review", mark: "optional" },
    { number: 7, name: "Execute", mark: "optional" },
  ],
  primary: {
    id: "start-metaplan",
    kind: "start_metaplan",
    label: "Start a metaplan",
    target: null,
  },
  secondary: [],
  items: [],
});

const SAMPLE_SERVER = serverInfoSchema.parse({
  project: "Atlas",
  project_key: "atlas-help",
  port: 7272,
  remote: false,
  read_only: [],
  stopping: false,
  you: { public_id: "atlas-help-reader", name: "Alex" },
});

function ignoreAction(): void {}

export function GettingAroundScene() {
  const [store] = useState(() => {
    const sample = createStore();
    sample.set(checklistAtom, SAMPLE_CHECKLIST);
    sample.set(serverInfoAtom, SAMPLE_SERVER);
    return sample;
  });

  return (
    <Provider store={store}>
      <MemoryRouter initialEntries={["/files"]}>
        <div data-loom-plan inert>
          <PlanHeader sample onOpenSettings={ignoreAction} onOpenHelp={ignoreAction} />
        </div>
      </MemoryRouter>
    </Provider>
  );
}
