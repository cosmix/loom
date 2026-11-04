import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import draftsFixture from "@/plan/api/fixtures/kickoff-drafts.json";
import serverFixture from "@/plan/api/fixtures/server-info.json";
import { planMessageSchema } from "@/plan/api/schema";
import { kickoffDraftViewSchema, type KickoffDraftView } from "@/plan/api/schema-kickoff";
import { connectPlanSocket } from "@/plan/api/socket";
import { DraftsShelf } from "@/plan/home/drafts-shelf";
import { kickoffDraftsAtom } from "@/plan/state/atoms";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const named = kickoffDraftViewSchema.parse(draftsFixture.drafts[0]);

function mount(drafts: KickoffDraftView[]) {
  const store = createStore();
  store.set(kickoffDraftsAtom, drafts);
  return render(
    <Provider store={store}>
      <MemoryRouter>
        <DraftsShelf />
      </MemoryRouter>
    </Provider>,
  );
}

it("renders nothing without drafts", () => {
  const { container } = mount([]);
  expect(container.childElementCount).toBe(0);
});

it("lists drafts newest first, each a link to its screen with its state", () => {
  const older = { ...named, session: "a", name: "older", updated_at: "2026-11-01T10:00:00Z" };
  const newer = {
    ...named,
    session: "b",
    name: "newer",
    live: true,
    updated_at: "2026-11-04T10:00:00Z",
  };
  mount([older, newer]);

  const shelf = within(screen.getByRole("region", { name: "Unfinished" }));
  const links = shelf.getAllByRole("link");
  expect(links.map((link) => link.getAttribute("href"))).toEqual([
    "/new?session=b",
    "/new?session=a",
  ]);
  expect(within(links[0]!).getByText("open")).toBeTruthy();
  expect(within(links[1]!).getByText("set aside")).toBeTruthy();
  expect(within(links[0]!).getByText("Tests need isolation.")).toBeTruthy();
});

it("titles an unnamed draft Untitled with the date it started, and tolerates no problem", () => {
  mount([{ ...named, name: null, problem: null, started_at: "2026-11-04T09:30:00Z" }]);

  const link = screen.getByRole("link");
  const expected = new Date("2026-11-04T09:30:00Z").toLocaleDateString(undefined, {
    dateStyle: "short",
  });
  expect(link.textContent).toContain(`Untitled · ${expected}`);
  expect(link.textContent).not.toContain("Tests need isolation.");
});

it("shows only a problem's first line, through Untrusted", () => {
  mount([{ ...named, problem: "\n  First line‮ here\nSecond line" }]);

  const link = screen.getByRole("link");
  expect(link.textContent).toContain("First line⟨U+202E⟩ here");
  expect(link.textContent).not.toContain("Second line");
});

it("the kickoff_drafts frame is a plan message that rejects unknown fields", () => {
  expect(planMessageSchema.safeParse(draftsFixture).success).toBe(true);
  expect(planMessageSchema.safeParse({ ...draftsFixture, extra: 1 }).success).toBe(false);
  const draft = { ...draftsFixture.drafts[0]!, extra: 1 };
  expect(planMessageSchema.safeParse({ ...draftsFixture, drafts: [draft] }).success).toBe(false);
});

it("the socket replaces the drafts with each kickoff_drafts frame", async () => {
  class FakeSocket {
    static last: FakeSocket | undefined;
    onmessage: ((event: { data: string }) => void) | null = null;
    constructor() {
      FakeSocket.last = this;
    }
    send() {}
    close() {}
  }
  // The socket probes the server before it opens.
  vi.stubGlobal("fetch", async () => new Response(JSON.stringify(serverFixture)));
  const store = createStore();
  const WebSocketImpl = FakeSocket as unknown as typeof WebSocket;
  const handle = connectPlanSocket(store, { WebSocketImpl });
  await vi.waitFor(() => expect(FakeSocket.last).toBeDefined());
  const receive = (frame: unknown) => FakeSocket.last?.onmessage?.({ data: JSON.stringify(frame) });

  receive(draftsFixture);
  expect(store.get(kickoffDraftsAtom)).toEqual(draftsFixture.drafts);
  receive({ ...draftsFixture, drafts: [] });
  expect(store.get(kickoffDraftsAtom)).toEqual([]);
  handle.dispose();
});
