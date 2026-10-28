import { act, render, screen } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { createMemoryRouter, RouterProvider } from "react-router";

import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import {
  chatSnapshotSchema,
  type ChatSnapshot,
  type Item,
  type Turn,
} from "@/plan/api/schema-chat";
import { snapshotAtom } from "./model";
import { Transcript } from "./transcript";

export const fixture = chatSnapshotSchema.parse(snapshotFixture);
type ItemOf<T extends Item["type"]> = Extract<Item, { type: T }>;

/** The first fixture item of `type`, with `fields` overridden. */
export function item<T extends Item["type"]>(type: T, fields: Partial<ItemOf<T>> = {}): ItemOf<T> {
  const base = fixture.turns.flatMap((turn) => turn.items).find((entry) => entry.type === type)!;
  return { ...base, ...fields } as ItemOf<T>;
}

export function turn(items: Item[], fields: Partial<Turn> = {}): Turn {
  return { ...fixture.turns[0]!, open: true, changed_files: [], items, ...fields };
}

export function mount(items: Item[] = [], fields: Partial<ChatSnapshot> = {}) {
  const store = createStore();
  const state: ChatSnapshot = {
    ...fixture,
    previous: null,
    queue: [],
    session: { ...fixture.session!, state: "working" },
    turns: [turn(items)],
    ...fields,
  };
  store.set(snapshotAtom, state);
  const router = createMemoryRouter([{ path: "*", element: <Transcript /> }], {
    initialEntries: ["/files/elsewhere.md"],
  });
  const view = render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  const update = (turns: Turn[]) =>
    act(() => store.set(snapshotAtom, (value) => ({ ...value, turns })));
  return { ...view, store, router, update, log: screen.getByRole("log") };
}
