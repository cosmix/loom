import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { useState } from "react";
import { Route, Routes } from "react-router";

import { checklistViewSchema } from "@/plan/api/schema";
import { sessionViewSchema } from "@/plan/api/schema-chat";
import { snapshotAtom } from "@/plan/chat/model";
import { Spine } from "@/plan/feature/spine";
import { PlanHeader } from "@/plan/header/header";
import { ignoreAction, SAMPLE_SERVER, SAMPLE_STEPS } from "@/plan/help/sample";
import { SceneRouter } from "@/plan/help/scene-router";
import { checklistAtom, serverInfoAtom } from "@/plan/state/atoms";

const SAMPLE_CHECKLIST = checklistViewSchema.parse({
  feature: "atlas",
  pitch: null,
  has_product: true,
  sections: [],
  plans: { listed: 0, total: 0, written: 0, running: 0, done: 0 },
  untested: 0,
  metaplans: [],
  plan_rows: [],
  current: { kind: "step", number: 3 },
  steps: SAMPLE_STEPS,
  primary: {
    id: "start-metaplan",
    kind: "start_metaplan",
    label: "Start the metaplan",
    target: null,
  },
  secondary: [],
  items: [],
});

// An idle planner chat is the one activity that needs only atoms: the pill reads "Waiting for you".
const SAMPLE_SESSION = sessionViewSchema.parse({
  id: "header-help",
  kind: "planner",
  state: "idle",
  name: "Atlas planning",
  model: "Claude",
  socket: "header-help.sock",
  holder: null,
  away_until: null,
  queued: 0,
  started_at: "2026-10-29T10:00:00Z",
  state_since: "2026-10-29T10:00:00Z",
  working_since: null,
  subagents_running: 0,
  last_activity: null,
  failure: null,
  offer: null,
  feature: null,
  plan: null,
});

export function GettingAroundScene() {
  const [store] = useState(() => {
    const sample = createStore();
    sample.set(checklistAtom, SAMPLE_CHECKLIST);
    sample.set(serverInfoAtom, SAMPLE_SERVER);
    sample.set(snapshotAtom, { session: SAMPLE_SESSION, turns: [], queue: [], previous: null });
    return sample;
  });

  return (
    <Provider store={store}>
      <SceneRouter initialEntries={["/f/atlas"]}>
        {/* Keep the illustrated feature visible on any app route without nesting a router. The spine
            reads its feature from the route, so it renders inside a `:feature` route. */}
        <Routes location="/f/atlas">
          <Route
            path="/f/:feature/*"
            element={
              <div data-loom-plan inert>
                <PlanHeader sample onOpenSettings={ignoreAction} onOpenHelp={ignoreAction} />
                <div className="w-64 max-w-full">
                  <Spine />
                </div>
              </div>
            }
          />
        </Routes>
      </SceneRouter>
    </Provider>
  );
}
