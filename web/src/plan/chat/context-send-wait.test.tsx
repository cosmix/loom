import { act } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { projectAtom } from "@/plan/state/atoms";
import { installComposerLifecycle, setup } from "./composer-harness";
import {
  installDocServer,
  pathOf,
  pathsAt,
  sender,
  sentAt,
  type Harness,
} from "./composer-context-kit";
import { NAMING_LIMIT_MS } from "./context-attach";
import { draftAtom } from "./drafts";
import { screenAtom } from "./session-home";
import { contextDismissedAtom, viewedDocumentAtom } from "./viewed-document";

installComposerLifecycle();
installDocServer();

// Fake timers: each test below proves the send left on the event it names, well before the cap
// (`NAMING_LIMIT_MS`) would have released it; the cap has its own test.
beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

// A direct load of a page shows the context chip from the route at once; the source path comes
// with the project frame. A message sent in between must still carry what the chip names.

const NO_PROJECT = { checklists: [], standalone: [], moves: [] };

function viewWithoutSource(h: Harness): void {
  act(() => h.store.set(viewedDocumentAtom, { kind: "section", path: null, name: "section_05" }));
  act(() => h.store.set(screenAtom, { feature: "atlas", plan: null }));
}

function name05(h: Harness): void {
  act(() =>
    h.store.set(viewedDocumentAtom, { kind: "section", path: pathOf("05"), name: "section_05" }),
  );
}

async function press(h: Harness, hook: ReturnType<typeof sender>): Promise<void> {
  act(() => h.store.set(draftAtom, (draft) => ({ ...draft, text: "Early question" })));
  await act(async () => {
    void hook.result.current.send();
  });
}

/** Lets `ms` of fake time pass, with the microtasks each timer releases. */
const pass = (ms: number) => act(() => vi.advanceTimersByTimeAsync(ms));

it("a send pressed while the chip waits for its source carries the document once it is named", async () => {
  const h = setup("idle");
  viewWithoutSource(h);
  const hook = sender(h);
  await press(h, hook);
  await pass(50);
  expect(h.sent).toHaveLength(0);

  name05(h);
  await pass(50);
  expect(h.sent).toHaveLength(1);
  expect(pathsAt(h, 0)).toEqual([pathOf("05")]);
  expect(sentAt(h, 0).text).toBe("Early question");
});

it("a send waiting for the source goes without it when the chip is dismissed", async () => {
  const h = setup("idle");
  viewWithoutSource(h);
  const hook = sender(h);
  await press(h, hook);
  expect(h.sent).toHaveLength(0);

  act(() => h.store.set(contextDismissedAtom, { path: null }));
  await pass(50);
  expect(h.sent).toHaveLength(1);
  expect(pathsAt(h, 0)).toEqual([]);
});

it("a send whose project arrives with no source for the page goes at once, not at the cap", async () => {
  const h = setup("idle");
  viewWithoutSource(h);
  const hook = sender(h);
  await press(h, hook);
  await pass(500);
  expect(h.sent).toHaveLength(0);

  act(() => h.store.set(projectAtom, NO_PROJECT));
  await pass(200);
  expect(h.sent).toHaveLength(1);
  expect(pathsAt(h, 0)).toEqual([]);
});

it("a path that follows the project by a frame is still carried", async () => {
  const h = setup("idle");
  viewWithoutSource(h);
  const hook = sender(h);
  await press(h, hook);

  act(() => h.store.set(projectAtom, NO_PROJECT));
  await pass(20);
  name05(h);
  await pass(200);
  expect(pathsAt(h, 0)).toEqual([pathOf("05")]);
});

it("a send that never gets a path goes without it at the cap, and the composer is free again", async () => {
  const h = setup("idle");
  viewWithoutSource(h);
  const hook = sender(h);
  await press(h, hook);
  await pass(NAMING_LIMIT_MS - 1);
  expect(h.sent).toHaveLength(0);
  expect(hook.result.current.pending).toBe(true);

  await pass(1);
  expect(h.sent).toHaveLength(1);
  expect(pathsAt(h, 0)).toEqual([]);
  await h.result();
  await pass(0);
  expect(hook.result.current.pending).toBe(false);
});

it("a route change during the wait attaches nothing from the page it moved to", async () => {
  const h = setup("idle");
  viewWithoutSource(h);
  const hook = sender(h);
  await press(h, hook);

  act(() => {
    h.store.set(screenAtom, { feature: "other", plan: null });
    h.store.set(viewedDocumentAtom, {
      kind: "metaplan",
      path: "doc/product/other/metaplan.md",
      name: "metaplan",
    });
  });
  await pass(50);
  expect(h.sent).toHaveLength(1);
  expect(pathsAt(h, 0)).toEqual([]);
});

it("the same page name on another feature attaches nothing either", async () => {
  const h = setup("idle");
  viewWithoutSource(h);
  const hook = sender(h);
  await press(h, hook);

  act(() => h.store.set(screenAtom, { feature: "other", plan: null }));
  name05(h);
  await pass(50);
  expect(h.sent).toHaveLength(1);
  expect(pathsAt(h, 0)).toEqual([]);
});
