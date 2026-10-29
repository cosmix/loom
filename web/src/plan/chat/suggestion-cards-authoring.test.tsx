import { act, screen } from "@testing-library/react";
import { expect, it } from "vitest";

import { authoringAtom } from "@/plan/state/atoms";
import { installSuggestionLifecycle, mount, setup, suggestion } from "./suggestion-cards-test-kit";

installSuggestionLifecycle();

it("a holder who frees and retakes authoring does not get the old card back", () => {
  const { store, receive } = setup();
  mount(store);
  receive(suggestion);
  expect(screen.getByText("Suggestions (1)")).toBeTruthy();
  const me = { public_id: "me", name: "Author", away_since: null, frees_at: null };

  act(() => store.set(authoringAtom, { holder: null }));
  act(() => store.set(authoringAtom, { holder: me }));

  expect(screen.queryByText("Suggestions (1)")).toBeNull();
  expect(screen.queryByRole("button", { name: "Attach" })).toBeNull();
});

it("a suggestion sent after authoring changed hands is shown", () => {
  const { store, receive } = setup();
  mount(store);
  act(() => store.set(authoringAtom, { holder: null }));
  act(() =>
    store.set(authoringAtom, {
      holder: { public_id: "me", name: "Author", away_since: null, frees_at: null },
    }),
  );

  receive(suggestion);

  expect(screen.getByText("Suggestions (1)")).toBeTruthy();
});
