import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import type { ReactNode } from "react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, beforeEach, vi } from "vitest";

import docFixture from "@/plan/api/fixtures/files-doc.json";
import serverFixture from "@/plan/api/fixtures/server-info.json";
import { serverInfoSchema } from "@/plan/api/schema";
import { planSocketAtom, serverInfoAtom } from "@/plan/state/atoms";
import { CHANGES_PATH, ChangedSinceControl } from "./changed-since";
import { ChangesPage } from "./changes-page";
import { checkpointAtom, type Checkpoint } from "./checkpoint";
import { hashesAtom, useReviewLive } from "./live";

export const A = "doc/product/a.md";
export const ADDED = "doc/product/added.md";
export const GONE = "doc/product/gone.md";
export const SAME = "doc/product/same.md";
export const REF = "doc/product/ref-only.md";
export const checkpoint: Checkpoint = {
  commit: "c1",
  hashes: { [A]: "old", [GONE]: "gone", [SAME]: "same" },
};
export const current = { head: "c2", docs: { [A]: "new", [ADDED]: "added", [SAME]: "same" } };
export const server = serverInfoSchema.parse(serverFixture);

/** Bodies served by the fetch stub, by request URL; reset before each test. */
export const replies: Record<string, unknown> = {};
/** Requests answered with an error instead of a body, by request URL; reset before each test. */
export const failures: Record<string, { status: number; message: string }> = {};
export const fetchMock = vi.fn(async (input: string) => {
  const url = new URL(input, "http://localhost");
  const failed = failures[input];
  if (failed)
    return new Response(JSON.stringify({ error: "refused", message: failed.message }), {
      status: failed.status,
    });
  let body = replies[input];
  if (url.pathname === "/api/at-commit") {
    body = { path: url.searchParams.get("path"), text: "committed text" };
  }
  if (url.pathname === "/api/doc") {
    const path = `doc/${url.searchParams.get("path")}`;
    if (path === GONE) return new Response("{}", { status: 404 });
    body = { ...docFixture, path, text: "current text", hash: "new" };
  }
  return new Response(JSON.stringify(body ?? {}));
});
let project = 0;

/** Registers the per-test reset; call once at the top of each changed-since test file. */
export function installChangedSinceLifecycle() {
  beforeEach(() => {
    localStorage.clear();
    for (const key of Object.keys(failures)) delete failures[key];
    for (const key of Object.keys(replies)) delete replies[key];
    Object.assign(replies, {
      "/api/hashes": current,
      "/api/open-questions": { counts: {} },
      "/api/compare?commit=c1": { commit: "c1", available: true, docs: [] },
      "/api/ref?name=main": { commit: "base" },
      "/api/compare?commit=base": {
        commit: "base",
        available: true,
        docs: [{ path: REF, change: "changed" }],
      },
    });
    fetchMock.mockClear();
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    localStorage.clear();
  });
}

function Live() {
  useReviewLive();
  return null;
}

/**
 * Renders the popover's trigger beside the router's outlet, at `/changes` unless `path` names
 * another page; `wrap` places the trigger (inside a popover, say).
 */
export function mount(
  options: {
    checkpoint?: Checkpoint | null;
    live?: boolean;
    path?: string;
    wrap?: (trigger: ReactNode) => ReactNode;
  } = {},
) {
  const store = createStore();
  const projectKey = `changed-since-${++project}`;
  store.set(serverInfoAtom, { ...server, project_key: projectKey });
  if (options.checkpoint !== null) store.set(checkpointAtom, options.checkpoint ?? checkpoint);
  if (!options.live) store.set(hashesAtom, current);
  const listeners = new Set<() => void>();
  const subscribers = new Set<{ type: string; handler: (message: never) => void }>();
  const socket = {
    send: vi.fn(() => true),
    subscribe: vi.fn((type: string, handler: (message: never) => void) => {
      const entry = { type, handler };
      subscribers.add(entry);
      return () => {
        subscribers.delete(entry);
      };
    }),
    dispose: vi.fn(),
    onResync: (handler: () => void) => {
      listeners.add(handler);
      return () => {
        listeners.delete(handler);
      };
    },
  };
  store.set(planSocketAtom, socket);
  const view = render(
    <Provider store={store}>
      <MemoryRouter initialEntries={[options.path ?? CHANGES_PATH]}>
        {options.live && <Live />}
        {options.wrap ? options.wrap(<ChangedSinceControl />) : <ChangedSinceControl />}
        <Routes>
          <Route path={CHANGES_PATH} element={<ChangesPage />} />
          <Route path="*" element={null} />
        </Routes>
      </MemoryRouter>
    </Provider>,
  );
  /** Delivers a socket frame to the subscribers of its type, as the live socket would. */
  const emit = (message: { type: string; [field: string]: unknown }) => {
    for (const { type, handler } of subscribers) {
      if (type === message.type) (handler as (message: unknown) => void)(message);
    }
  };
  return { store, projectKey, listeners, view, emit };
}

/** Resolves once the page lists the changed documents and A's diff can be opened. */
export async function openList() {
  await screen.findByRole("list", { name: "Changed documents" });
  await screen.findByRole("button", { name: `Show diff for ${A}` });
  return screen.getByRole("list", { name: "Changed documents" });
}

export async function compareRef() {
  fireEvent.change(screen.getByLabelText("Compare with ref"), { target: { value: "main" } });
  fireEvent.click(screen.getByRole("button", { name: "Compare" }));
  return screen.findByText(REF);
}
