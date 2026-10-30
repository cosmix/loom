import { act, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import authoringFixture from "@/plan/api/fixtures/authoring.json";
import { authoringViewSchema } from "@/plan/api/schema";
import { sessionViewSchema } from "@/plan/api/schema-chat";
import { pendingAttachmentsAtom } from "@/plan/attachments";
import { snapshotAtom } from "@/plan/chat/model";
import { screenAtom } from "@/plan/chat/session-home";
import { authoringAtom } from "@/plan/state/atoms";

import { canAuthor, clone, makeStubFetch, readingColumn, renderAt } from "./concept-route-support";

const HINT = /adds the sentence to chat/;
const session = sessionViewSchema.parse({ ...snapshotFixture.session, feature: "elsewhere" });

beforeEach(() => {
  const pages = { "overview/demo/product": clone(overviewFixture) };
  vi.stubGlobal("fetch", vi.fn(makeStubFetch(() => ({ pages, docs: {}, failing: false }), [])));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("a chat that belongs to another screen", () => {
  it("lets `a` attach and shows its hint once the chat ended, not while it is open", async () => {
    const { store } = renderAt("/f/demo/product", (setup) => {
      canAuthor(setup);
      const open = { ...session, state: "idle" as const };
      setup.set(snapshotAtom, { session: open, turns: [], queue: [], previous: null });
      setup.set(screenAtom, { feature: "demo", plan: null });
    });
    const column = await readingColumn();
    column.focus();
    await waitFor(() => expect(column.contains(document.activeElement)).toBe(true));
    fireEvent.keyDown(column, { key: "a" });
    expect(store.get(pendingAttachmentsAtom)).toHaveLength(0);
    expect(screen.queryByText(HINT)).toBeNull();

    const ended = { ...session, state: "ended" as const };
    store.set(snapshotAtom, {
      session: null,
      turns: [],
      queue: [],
      previous: { session: ended, turns: [] },
    });
    fireEvent.keyDown(column, { key: "a" });
    await waitFor(() => expect(store.get(pendingAttachmentsAtom)).toHaveLength(1));
    expect(screen.getByText(HINT)).toBeTruthy();
  });
});

describe("the reading keys while authoring changes", () => {
  it("keep the column and the key row, and the attach hint follows the flip", async () => {
    const { store } = renderAt("/f/demo/product", canAuthor);
    await readingColumn();
    expect(screen.getByRole("group", { name: "Reading keys" })).toBeTruthy();
    expect(screen.getByText(HINT)).toBeTruthy();

    // Another browser takes authoring, then it frees up again: the hook count must not change.
    act(() => store.set(authoringAtom, authoringViewSchema.parse(authoringFixture)));
    expect(await readingColumn()).toBeTruthy();
    expect(screen.getByRole("group", { name: "Reading keys" })).toBeTruthy();
    expect(screen.queryByText(HINT)).toBeNull();

    act(() =>
      store.set(authoringAtom, authoringViewSchema.parse({ ...authoringFixture, holder: null })),
    );
    expect(await readingColumn()).toBeTruthy();
    expect(screen.getByRole("group", { name: "Reading keys" })).toBeTruthy();
    expect(screen.getByText(HINT)).toBeTruthy();
  });
});
