import { render } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { MemoryRouter } from "react-router";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import planFixture from "@/plan/api/fixtures/files-doc-plan.json";
import hello from "@/plan/api/fixtures/hello.json";
import { checklistViewSchema, serverInfoSchema } from "@/plan/api/schema";
import { docViewSchema, type DocView, type PlanView } from "@/plan/api/schema-files";
import { authoringAtom, projectAtom, serverInfoAtom } from "@/plan/state/atoms";

import { FileDocHeading } from "./doc-heading-view";

export function makeDoc(patch: Partial<DocView> = {}, plan: Partial<PlanView> = {}): DocView {
  const base = docViewSchema.parse(planFixture);
  return { ...base, plan: { ...base.plan!, ...plan }, ...patch };
}

export function authorStore(canAuthor = true) {
  const store = createStore();
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...hello.server, read_only: [] }));
  store.set(authoringAtom, canAuthor ? { holder: null } : hello.authoring);
  return store;
}

export function withFeatureTitle(
  store: ReturnType<typeof authorStore>,
  feature: string,
  title: string,
) {
  const checklist = checklistViewSchema.parse(structuredClone(checklistFixture.checklists[0]));
  store.set(projectAtom, {
    checklists: [{ ...checklist, feature, title }],
    standalone: [],
    moves: [],
  });
  return store;
}

export function mount(
  doc = makeDoc(),
  store = authorStore(),
  address = "/files/plans/demo/PLAN-file-mode.md",
) {
  return {
    store,
    ...render(
      <Provider store={store}>
        <MemoryRouter initialEntries={[address]}>
          <FileDocHeading doc={doc} />
        </MemoryRouter>
      </Provider>,
    ),
  };
}
