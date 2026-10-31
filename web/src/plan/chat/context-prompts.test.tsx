import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { afterEach, expect, it } from "vitest";

import fixture from "@/plan/api/fixtures/chat-snapshot.json";
import { chatSnapshotSchema } from "@/plan/api/schema-chat";
import { source } from "./composer-harness";
import { focusComposerAtom } from "./composer-hooks";
import { ContextPrompts, promptsOf } from "./context-prompts";
import { draftAtom } from "./drafts";
import { snapshotAtom } from "./model";
import { contextDismissedAtom, viewedDocumentAtom, type ViewedKind } from "./viewed-document";

afterEach(cleanup);

const PROMPTS: Array<[NonNullable<ViewedKind>, string[]]> = [
  ["vision", ["Which sections does this imply?", "What is still undecided?"]],
  ["product-overview", ["What are the main decisions here?", "What is still undecided?"]],
  [
    "section",
    [
      "What does this section decide?",
      "What does this conflict with?",
      "Change this section so that",
    ],
  ],
  ["metaplan", ["Is the plan order right?", "Which plan carries the most risk?"]],
  ["engineering-overview", ["What are the main engineering decisions?", "What could go wrong?"]],
  [
    "plan",
    [
      "What could go wrong with this plan?",
      "Does this cover what its sections ask for?",
      "Change this plan so that",
    ],
  ],
  ["pressure", ["Which plan should be tested first?"]],
  ["run", ["What did the build change compared to the spec?"]],
];

function mount(kind: ViewedKind, state: "idle" | "ended" | null = null) {
  const store = createStore();
  store.set(viewedDocumentAtom, { kind, path: null, name: "page" });
  const session = state ? { ...chatSnapshotSchema.parse(fixture).session!, state } : null;
  store.set(snapshotAtom, { session, turns: [], queue: [], previous: null });
  render(
    <Provider store={store}>
      <ContextPrompts />
    </Provider>,
  );
  return store;
}

const listed = () =>
  within(screen.getByRole("list", { name: "Suggested prompts" }))
    .getAllByRole("button")
    .map((button) => button.textContent);

it.each(PROMPTS)("a %s page suggests its prompts, in order", (kind, prompts) => {
  mount(kind);
  expect(listed()).toEqual(prompts);
  expect(promptsOf(kind)).toEqual(prompts);
});

it.each<ViewedKind>(["file", null])("a %s page suggests nothing", (kind) => {
  mount(kind);
  expect(screen.queryByRole("list", { name: "Suggested prompts" })).toBeNull();
});

it("shows after a concluded chat but not while a chat runs", () => {
  mount("section", "idle");
  expect(screen.queryByRole("list", { name: "Suggested prompts" })).toBeNull();
  cleanup();
  mount("section", "ended");
  expect(listed()).toHaveLength(3);
});

it("hides while the draft holds text or an attachment, and shows for blank space", () => {
  const store = mount("section");
  act(() => store.set(draftAtom, { text: "  \n", attachments: [] }));
  expect(listed()).toHaveLength(3);
  act(() => store.set(draftAtom, { text: "Already typing", attachments: [] }));
  expect(screen.queryByRole("list", { name: "Suggested prompts" })).toBeNull();
  act(() => store.set(draftAtom, { text: "", attachments: [source()] }));
  expect(screen.queryByRole("list", { name: "Suggested prompts" })).toBeNull();
});

it("a dismissed context takes its prompts with it", () => {
  const store = mount("section");
  act(() => store.set(contextDismissedAtom, { path: null }));
  expect(screen.queryByRole("list", { name: "Suggested prompts" })).toBeNull();
});

it("a click fills the draft and asks the composer to focus", () => {
  const store = mount("section");
  fireEvent.click(screen.getByRole("button", { name: "What does this conflict with?" }));
  expect(store.get(draftAtom)).toEqual({ text: "What does this conflict with?", attachments: [] });
  expect(store.get(focusComposerAtom)).toBe(true);
});
