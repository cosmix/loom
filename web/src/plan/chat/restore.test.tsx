import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { webcrypto } from "node:crypto";
import type { ReactNode } from "react";
import { MemoryRouter, useLocation } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import fixture from "@/plan/api/fixtures/chat-snapshot.json";
import { chatSnapshotSchema, type SessionView, type Turn } from "@/plan/api/schema-chat";
import { requestEditAtom } from "@/plan/editor/request-edit";
import { docChangedAtom } from "@/plan/files/atoms";
import { authoringAtom, youAtom } from "@/plan/state/atoms";

import { snapshotAtom } from "./model";
import { RestoreAll, RestoreButton } from "./restore";

const path = "doc/product/section_02.md";
const second = "doc/plans/PLAN-a #1.md";
const snapshot = chatSnapshotSchema.parse(fixture);
const baseSession = snapshot.session!;
const me = { public_id: "me", name: "Me" };
const turn: Turn = {
  ...snapshot.turns[0],
  open: false,
  changed_files: [path, second].map((path) => ({ path, before_hash: "before", by_tool: true })),
};
type Store = ReturnType<typeof createStore>;
type Body = { session: string; turn: string; paths: string[]; force: boolean };
let posts: Body[];
let response: unknown;
let status: number;
let diffs: Record<string, { before: string | null; after: string | null }>;
let fetchMock: ReturnType<typeof vi.fn<typeof fetch>>;

function setup(holder = true, session: Partial<SessionView> = {}, turns = [turn]) {
  const store = createStore();
  store.set(youAtom, me);
  store.set(authoringAtom, {
    holder: {
      ...(holder ? me : { public_id: "other", name: "Other" }),
      away_since: null,
      frees_at: null,
    },
  });
  store.set(snapshotAtom, {
    ...snapshot,
    session: {
      ...baseSession,
      state: "idle",
      subagents_running: 0,
      working_since: null,
      ...session,
    },
    turns,
  });
  return store;
}

function Location() {
  const location = useLocation();
  return (
    <output aria-label="Location">
      {location.pathname}
      {location.search}
    </output>
  );
}

function mount(
  store: Store,
  child: ReactNode = <RestoreButton sessionId={baseSession.id} turn={turn} path={path} />,
) {
  return render(
    <Provider store={store}>
      <MemoryRouter initialEntries={["/product"]}>
        {child}
        <Location />
      </MemoryRouter>
    </Provider>,
  );
}

async function clickRestore() {
  fireEvent.click(await screen.findByRole("button", { name: "Restore" }));
}

async function answer(input: RequestInfo | URL, init?: RequestInit) {
  const url = new URL(String(input), "http://localhost");
  if (url.pathname === "/api/restore") {
    posts.push(JSON.parse(String(init?.body)) as Body);
    return new Response(JSON.stringify(response), { status });
  }
  const relative = url.searchParams.get("path") ?? "";
  const diff = diffs[`doc/${relative}`] ?? { before: "before", after: "hello \t" };
  return new Response(JSON.stringify({ path: `doc/${relative}`, ...diff }));
}

beforeEach(() => {
  posts = [];
  response = { files: [{ path, result: "restored" }] };
  status = 200;
  diffs = {};
  fetchMock = vi.fn<typeof fetch>(answer);
  vi.stubGlobal("fetch", fetchMock);
  vi.stubGlobal("crypto", webcrypto);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("viewer sees no restore", () => {
  mount(
    setup(false),
    <>
      <RestoreButton sessionId={baseSession.id} turn={turn} path={path} />
      <RestoreAll sessionId={baseSession.id} turn={turn} paths={[path, second]} />
    </>,
  );

  expect(screen.queryByRole("button", { name: /restore/i })).toBeNull();
  expect(fetchMock).not.toHaveBeenCalled();
});

it("hidden while working", () => {
  mount(
    setup(true, { state: "working" }),
    <>
      <RestoreButton sessionId={baseSession.id} turn={turn} path={path} />
      <RestoreAll sessionId={baseSession.id} turn={turn} paths={[path, second]} />
    </>,
  );

  expect(screen.queryByRole("button", { name: /restore/i })).toBeNull();
});

it.each(["starting", "permission", "ending"] as const)("hidden during %s", (state) => {
  mount(setup(true, { state }));

  expect(screen.queryByRole("button", { name: /restore/i })).toBeNull();
});

it("hidden while another turn or subagent runs", () => {
  const store = setup(true, {}, [turn, { ...turn, id: "t2", open: true }]);
  mount(store);
  expect(screen.queryByRole("button", { name: /restore/i })).toBeNull();

  act(() =>
    store.set(snapshotAtom, {
      ...store.get(snapshotAtom),
      turns: [turn],
      session: { ...baseSession, state: "idle", subagents_running: 1 },
    }),
  );
  expect(screen.queryByRole("button", { name: /restore/i })).toBeNull();
});

it("hidden for an open turn missing from the snapshot", () => {
  mount(
    setup(true, {}, []),
    <RestoreButton sessionId={baseSession.id} turn={{ ...turn, open: true }} path={path} />,
  );

  expect(screen.queryByRole("button", { name: /restore/i })).toBeNull();
});

it("no restore on new or deleted rows", async () => {
  const rows: Turn = {
    ...turn,
    changed_files: [
      { path, before_hash: null, by_tool: true },
      { path: second, before_hash: "before", by_tool: true },
    ],
  };
  diffs[second] = { before: "deleted text", after: null };
  mount(
    setup(true, {}, [rows]),
    <>
      <RestoreButton sessionId={baseSession.id} turn={rows} path={path} />
      <RestoreButton sessionId={baseSession.id} turn={rows} path={second} />
      <RestoreAll sessionId={baseSession.id} turn={rows} paths={[path, second]} />
    </>,
  );

  await waitFor(() => expect(fetchMock).toHaveBeenCalled());
  await act(async () => {});
  expect(screen.queryByRole("button", { name: /restore/i })).toBeNull();
  expect(posts).toEqual([]);
});

it("does not offer restore for unrelated or outside doc paths", async () => {
  const outside = {
    ...turn,
    changed_files: [{ path: "README.md", before_hash: "old", by_tool: true }],
  };
  mount(
    setup(),
    <>
      <RestoreButton sessionId={baseSession.id} turn={turn} path="doc/unrelated.md" />
      <RestoreButton sessionId={baseSession.id} turn={outside} path="README.md" />
    </>,
  );

  await act(async () => {});
  expect(screen.queryByRole("button", { name: /restore/i })).toBeNull();
  expect(fetchMock).not.toHaveBeenCalled();
});

it("changed opens merge view", async () => {
  const store = setup();
  response = {
    files: [
      {
        path,
        result: "changed",
        current_text: "someone else's text",
        current_hash: "current-hash",
        turn_start_text: "turn-start text",
      },
    ],
  };
  mount(store);

  await clickRestore();

  await waitFor(() =>
    expect(screen.getByLabelText("Location").textContent).toBe(
      "/files/product/section_02.md?edit=1",
    ),
  );
  expect(store.get(requestEditAtom)).toEqual({
    path: "product/section_02.md",
    text: "turn-start text",
    openedHash: "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
  });
  expect(posts).toEqual([{ session: baseSession.id, turn: turn.id, paths: [path], force: false }]);
});

it("invalid asks save with errors and resends force", async () => {
  response = { files: [{ path, result: "invalid", errors: ["Missing acceptance criteria"] }] };
  mount(setup());

  await clickRestore();
  const dialog = await screen.findByRole("dialog", { name: "Save with errors?" });
  expect(within(dialog).getByText("Missing acceptance criteria")).toBeTruthy();
  expect(posts).toHaveLength(1);
  response = { files: [{ path, result: "restored" }] };
  fireEvent.click(within(dialog).getByRole("button", { name: "Save with errors" }));

  await waitFor(() =>
    expect(posts).toEqual([
      { session: baseSession.id, turn: turn.id, paths: [path], force: false },
      { session: baseSession.id, turn: turn.id, paths: [path], force: true },
    ]),
  );
  expect(await screen.findByText(`Restored ${path}`)).toBeTruthy();
});

it("restore request failure shows the reason", async () => {
  status = 503;
  response = { error: "unavailable", message: "The pinned turn text is unavailable" };
  mount(setup());

  await clickRestore();

  expect((await screen.findByRole("alert")).textContent).toBe(
    "The pinned turn text is unavailable",
  );
});

it("refused shows the reason safely", async () => {
  response = { files: [{ path, result: "refused", reason: "Locked \u202E<script>" }] };
  mount(setup());

  await clickRestore();

  const alert = await screen.findByRole("alert");
  expect(alert.textContent).toContain("Locked ⟨U+202E⟩<script>");
  expect(alert.querySelector("script")).toBeNull();
});

it("restore all asks first and lists repo-relative paths", async () => {
  mount(setup(), <RestoreAll sessionId={baseSession.id} turn={turn} paths={[path, second]} />);

  fireEvent.click(await screen.findByRole("button", { name: "Restore all" }));
  const dialog = await screen.findByRole("dialog", { name: "Restore all files?" });
  expect(dialog.textContent).toContain(path);
  expect(dialog.textContent).toContain(second);
  expect(posts).toEqual([]);
  fireEvent.click(within(dialog).getByRole("button", { name: /restore/i }));

  await waitFor(() =>
    expect(posts).toEqual([
      { session: baseSession.id, turn: turn.id, paths: [path, second], force: false },
    ]),
  );
});

it("restore all keeps focus on its button after confirming, and stays closed while restoring", async () => {
  // The restore request never answers, so the action stays pending after the dialog closes.
  fetchMock.mockImplementation((input, init) =>
    String(input).includes("/api/restore") ? new Promise<Response>(() => {}) : answer(input, init),
  );
  mount(setup(), <RestoreAll sessionId={baseSession.id} turn={turn} paths={[path, second]} />);
  const trigger = await screen.findByRole("button", { name: "Restore all" });
  trigger.focus();
  fireEvent.click(trigger);
  const dialog = await screen.findByRole("dialog", { name: "Restore all files?" });
  fireEvent.click(within(dialog).getByRole("button", { name: "Restore files" }));

  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  // `aria-disabled`, not `disabled`: a disabled button cannot take the focus Radix hands back.
  expect(trigger.getAttribute("aria-disabled")).toBe("true");
  expect(trigger.hasAttribute("disabled")).toBe(false);
  await waitFor(() => expect(document.activeElement).toBe(trigger));
  fireEvent.click(trigger);
  expect(screen.queryByRole("dialog")).toBeNull();
});

it("restore all cancel sends nothing", async () => {
  mount(setup(), <RestoreAll sessionId={baseSession.id} turn={turn} paths={[path, second]} />);
  fireEvent.click(await screen.findByRole("button", { name: "Restore all" }));
  const dialog = await screen.findByRole("dialog");

  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

  expect(screen.queryByRole("dialog")).toBeNull();
  expect(posts).toEqual([]);
});

it("losing authoring or starting work closes confirmation without posting", async () => {
  const store = setup();
  mount(store, <RestoreAll sessionId={baseSession.id} turn={turn} paths={[path, second]} />);
  fireEvent.click(await screen.findByRole("button", { name: "Restore all" }));
  expect(await screen.findByRole("dialog")).toBeTruthy();

  act(() => store.set(authoringAtom, { holder: null }));

  expect(screen.queryByRole("dialog")).toBeNull();
  expect(posts).toEqual([]);
});

it("deleted documents disappear from restore all", async () => {
  const store = setup();
  store.set(docChangedAtom, { [second]: null });
  mount(store, <RestoreAll sessionId={baseSession.id} turn={turn} paths={[path, second]} />);

  await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
  await act(async () => {});
  expect(screen.queryByRole("button", { name: "Restore all" })).toBeNull();
});

it("uses the footer's texts instead of fetching them again", async () => {
  const texts = [path, second].map((name) => ({
    path: name,
    texts: { path: name, before: "before", after: "after" },
  }));
  mount(
    setup(),
    <RestoreAll sessionId={baseSession.id} turn={turn} paths={[path, second]} texts={texts} />,
  );

  expect(await screen.findByRole("button", { name: "Restore all" })).toBeTruthy();
  expect(fetchMock).not.toHaveBeenCalled();
});

it("shared texts without a row yet offer nothing", async () => {
  mount(setup(), <RestoreButton sessionId={baseSession.id} turn={turn} path={path} texts={[]} />);

  await act(async () => {});
  expect(screen.queryByRole("button", { name: "Restore" })).toBeNull();
  expect(fetchMock).not.toHaveBeenCalled();
});
