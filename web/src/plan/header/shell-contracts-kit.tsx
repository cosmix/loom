import { render } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createStore } from "jotai/vanilla";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router";
import { vi } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import { checklistViewSchema, serverInfoSchema, type ChecklistView } from "@/plan/api/schema";
import { checklistAtom, serverInfoAtom } from "@/plan/state/atoms";

type Store = ReturnType<typeof createStore>;

/** Matches the planner's `lg` query, so the header and dock take their desktop layout. */
export function desktopMedia(): void {
  vi.spyOn(window, "matchMedia").mockImplementation((query) => ({
    matches: query === "(min-width: 1024px)",
    media: query,
    onchange: null,
    addEventListener() {},
    removeEventListener() {},
    addListener() {},
    removeListener() {},
    dispatchEvent: () => false,
  }));
}

/** The fixture checklist for feature "demo", with optional step overrides. */
export function demoChecklist(patch: Partial<ChecklistView> = {}): ChecklistView {
  return checklistViewSchema.parse({ ...checklistFixture.checklists[0]!, ...patch });
}

/** A store holding the demo checklist and the fixture server info. */
export function demoStore(): Store {
  const store = createStore();
  store.set(checklistAtom, demoChecklist());
  store.set(serverInfoAtom, serverInfoSchema.parse(serverInfoFixture));
  return store;
}

export function renderAt(pathname: string, ui: ReactNode, store: Store = createStore()) {
  return render(
    <Provider store={store}>
      <MemoryRouter initialEntries={[pathname]}>
        <div data-loom-plan>{ui}</div>
      </MemoryRouter>
    </Provider>,
  );
}
