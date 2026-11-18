import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createStore, getDefaultStore, Provider } from "jotai";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, vi } from "vitest";

import fixture from "@/plan/api/fixtures/chat-snapshot.json";
import { chatSnapshotSchema, type Item, type SessionState } from "@/plan/api/schema-chat";
import type { connectPlanSocket } from "@/plan/api/socket";
import type { AttachmentDraft } from "@/plan/attachments";
import {
  authoringAtom,
  connectionAtom,
  planSocketAtom,
  serverInfoAtom,
  youAtom,
} from "@/plan/state/atoms";
import { ComposerArea } from "./composer";
import { useComposerShortcut } from "./composer-hooks";
import { installChatSubscription, snapshotAtom } from "./model";
import { storageFullAtom } from "./storage";
import { useQueueRefill } from "./stop-control";

type Store = ReturnType<typeof createStore>;
export const initial = chatSnapshotSchema.parse(fixture);
export const me = { public_id: "me", name: "Me" };
export const other = { public_id: "ada", name: "Ada", away_since: null, frees_at: null };
const cleanups: (() => void)[] = [];
let project = 0;

function seed(store: Store, state: SessionState | null, viewer: boolean) {
  store.set(serverInfoAtom, {
    project: "p",
    project_key: `composer-${++project}`,
    port: 1,
    remote: false,
    stopping: false,
    read_only: [],
    you: me,
  });
  store.set(youAtom, me);
  store.set(authoringAtom, { holder: viewer ? other : { ...other, ...me } });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: null });
  store.set(snapshotAtom, {
    ...initial,
    session: state ? { ...initial.session!, state } : null,
    turns: [],
    queue: [],
    previous: null,
  });
}

function fakeSocket() {
  const listeners = new Map<string, Set<(frame: unknown) => void>>();
  const sent: Record<string, unknown>[] = [];
  const socket = {
    send: (message: Record<string, unknown>) => {
      sent.push(message);
      return true;
    },
    subscribe: (type: string, fn: (frame: unknown) => void) => {
      const handlers = listeners.get(type) ?? new Set();
      listeners.set(type, handlers);
      handlers.add(fn);
      return () => {
        handlers.delete(fn);
      };
    },
    onResync: () => () => {},
    dispose: () => {},
  };
  const receive = (type: string, frame: object) =>
    act(() => {
      [...(listeners.get(type) ?? [])].forEach((fn) => fn({ type, ...frame }));
    });
  return { socket: socket as unknown as ReturnType<typeof connectPlanSocket>, sent, receive };
}

export function setup(state: SessionState | null = "idle", viewer = false) {
  const store = createStore();
  seed(store, state, viewer);
  const { socket, sent, receive } = fakeSocket();
  store.set(planSocketAtom, socket);
  cleanups.push(installChatSubscription(store));
  const result = async (error: string | null = null, index = sent.length - 1) => {
    await act(async () =>
      receive("chat", {
        frame: "result",
        client_id: sent[index].client_id,
        ok: error === null,
        error,
      }),
    );
  };
  return { store, sent, receive, result };
}
/** Registers the `c` key the way ChatRegistrations does, beside a composer that does not. */
export function ShortcutHost(): null {
  useComposerShortcut();
  return null;
}
/** Runs the queue refill beside the composer, as ChatRegistrations does in the page. */
function QueueRefill({ children }: { children: ReactNode }) {
  useQueueRefill();
  return <>{children}</>;
}
export function mount(store: Store, child = <ComposerArea />) {
  return render(
    <Provider store={store}>
      <MemoryRouter>
        <QueueRefill>{child}</QueueRefill>
      </MemoryRouter>
    </Provider>,
  );
}
export const box = () => screen.getByRole<HTMLTextAreaElement>("textbox", { name: "Message" });
export const send = () => screen.getByRole("button", { name: "Send" });
export const type = (text = "Hello") => fireEvent.change(box(), { target: { value: text } });
export const queue = (store: Store, count = 3) =>
  store.set(snapshotAtom, (s) => ({
    ...s,
    queue: Array.from({ length: count }, (_, i) => ({
      id: `${i}`,
      first_line: `first ${i}`,
      text: `first ${i}\nsecond ${i}`,
    })),
  }));
export const source = (n = 0): AttachmentDraft => ({
  kind: "source",
  path: `doc/product/note-${n}.md`,
  anchor: null,
  lines: null,
  stageId: null,
  hash: null,
  text: "Selected passage",
});
export function permissions(store: Store, count = 2) {
  const items: Item[] = Array.from({ length: count }, (_, i) => ({
    type: "permission",
    id: `p${i}`,
    request_id: `p${i}`,
    tool: `Bash ${i}`,
    input: "echo hello",
    status: "pending",
    deny_reason: null,
    note: null,
    expires_at: "2099-01-01T00:00:00Z",
    fetch: null,
  }));
  store.set(snapshotAtom, (s) => ({ ...s, turns: [{ ...initial.turns[0], items }] }));
}
/** Registers the per-test reset; call once at the top of each composer test file. */
export function installComposerLifecycle() {
  beforeEach(() => {
    localStorage.clear();
    getDefaultStore().set(storageFullAtom, false);
  });
  afterEach(() => {
    cleanup();
    cleanups.splice(0).forEach((off) => off());
    vi.restoreAllMocks();
    localStorage.clear();
  });
}
