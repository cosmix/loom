import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import commandsFixture from "@/plan/api/fixtures/chat-commands.json";
import deltasFixture from "@/plan/api/fixtures/chat-deltas.json";
import framesFixture from "@/plan/api/fixtures/chat-frames.json";
import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import treeFixture from "@/plan/api/fixtures/files-tree.json";
import helloFixture from "@/plan/api/fixtures/hello.json";
import {
  attachmentInputSchema,
  chatCommandSchema,
  chatFrameSchema,
  chatSnapshotSchema,
  type ChatFrame,
} from "@/plan/api/schema-chat";
import { connectPlanSocket } from "@/plan/api/socket";
import {
  addAttachmentAtom,
  pendingAttachmentsAtom,
  type AttachmentDraft,
} from "@/plan/attachments";
import { docRel } from "@/plan/files/address";
import { routes } from "@/plan/router";
import { planSocketAtom } from "@/plan/state/atoms";

import { toAttachmentInput } from "./commands";
import { applyChatFrame } from "./model";

type Obj = Record<string, unknown>;
type Store = ReturnType<typeof createStore>;

const matchMedia = window.matchMedia;
const recordedSnapshot = chatSnapshotSchema.parse(snapshotFixture);
const recordedSend = chatCommandSchema.parse(
  commandsFixture.find((command) => command.command === "send"),
);
const passageInput = attachmentInputSchema.parse(commandsFixture[0]?.attachments?.[0]);
const passage: AttachmentDraft = {
  kind: "source",
  path: docRel(passageInput.path),
  anchor: passageInput.anchor,
  lines: passageInput.lines && { start: passageInput.lines[0], end: passageInput.lines[1] },
  stageId: passageInput.stage,
  text: passageInput.quote,
  hash: passageInput.selected_hash,
};
const sessionId = recordedSnapshot.session!.id;

// Use plan 2's transport seam while mounting the real application routes.
class FakeSocket {
  static last: FakeSocket | undefined;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onopen: ((event: Event) => void) | null = null;
  readyState = 0;
  sent: Obj[] = [];

  constructor() {
    FakeSocket.last = this;
  }
  send = (data: string): number => this.sent.push(JSON.parse(data) as Obj);
  close = (): number => (this.readyState = 3);
  open = (): void => {
    this.readyState = 1;
    this.onopen?.(new Event("open"));
  };
  receive = (frame: unknown): void =>
    this.onmessage?.({ data: JSON.stringify(frame) } as MessageEvent);
}

let replies: Record<string, unknown>;
let fetchMock = vi.fn<typeof fetch>();
const disposers: Array<() => void> = [];

function hello() {
  const base = structuredClone(helloFixture);
  const you = { public_id: "0123456789abcdef", name: "Ada" };
  return {
    ...base,
    you,
    server: { ...base.server, read_only: [], you },
    presence: { entries: [] },
    authoring: { holder: { ...you, away_since: null, frees_at: null } },
  };
}

function pathOf(input: RequestInfo | URL): string {
  const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
  return new URL(raw, "http://localhost").pathname;
}

async function answer(input: RequestInfo | URL): Promise<Response> {
  const path = pathOf(input);
  const body = path === "/api/doc" ? docFixture : replies[path];
  return new Response(JSON.stringify(body ?? { error: "not_found", message: "not found" }), {
    status: body === undefined ? 404 : 200,
    headers: { "content-type": "application/json" },
  });
}

function replay(store: Store, frame: unknown): void {
  const parsed = chatFrameSchema.parse(frame);
  act(() => applyChatFrame(store, parsed));
}

async function mount(): Promise<Store> {
  const store = createStore();
  initClient(store);
  const handle = connectPlanSocket(store, {
    WebSocketImpl: FakeSocket as unknown as typeof WebSocket,
  });
  disposers.push(handle.dispose);
  store.set(planSocketAtom, handle);
  const router = createMemoryRouter(routes, { initialEntries: ["/product"] });
  disposers.push(() => router.dispose());
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  await waitFor(() => expect(FakeSocket.last).toBeDefined());
  act(() => {
    FakeSocket.last!.open();
    FakeSocket.last!.receive(hello());
  });
  replay(store, {
    frame: "snapshot",
    ...recordedSnapshot,
    session: { ...recordedSnapshot.session, state: "idle", working_since: null, queued: 0 },
    turns: [],
    queue: [],
    previous: null,
  });
  return store;
}

// jsdom has no layout, so presence in the document stands in for visibility.
function present(element: HTMLElement): void {
  expect(element.isConnected).toBe(true);
}

async function sendPassage(store: Store, chat: HTMLElement): Promise<void> {
  if (recordedSend.command !== "send") throw new Error("Expected the recorded send command");
  act(() => store.set(addAttachmentAtom, passage));
  expect(store.get(pendingAttachmentsAtom)).toEqual([passage]);
  expect(toAttachmentInput(passage)).toEqual(passageInput);
  const message = await within(chat).findByRole("textbox", { name: "Message" });
  fireEvent.change(message, { target: { value: recordedSend.text } });
  fireEvent.click(within(chat).getByRole("button", { name: "Send" }));
  await waitFor(() =>
    expect(FakeSocket.last!.sent.filter((frame) => frame.type === "chat")).toEqual([
      { type: "chat", ...recordedSend, attachments: [passageInput] },
    ]),
  );

  // Command acknowledgements also pass through the socket's command subscription.
  const result = chatFrameSchema.parse(framesFixture[2]);
  act(() => FakeSocket.last!.receive({ type: "chat", ...result }));
  await waitFor(() => {
    expect((message as HTMLTextAreaElement).value).toBe("");
    expect(store.get(pendingAttachmentsAtom)).toEqual([]);
  });
}

async function replayItems(store: Store, chat: HTMLElement): Promise<void> {
  const working = deltasFixture[0];
  replay(store, { frame: "delta", ...working, session: { ...working?.session, id: sessionId } });
  replay(store, { frame: "delta", ...deltasFixture[1] });
  const user = recordedSnapshot.turns[0]!.items[0]!;
  const text = recordedSnapshot.turns[0]!.items.find((item) => item.type === "text")!;
  replay(store, { frame: "delta", op: "item_added", turn_id: "t1", item: user });
  const log = within(chat).getByRole("log");
  present(within(log).getByText("Please update the bridge plan.", { exact: true }));
  replay(store, { frame: "delta", op: "item_added", turn_id: "t1", item: text });
  present(within(log).getByText("I will update the plan and check the result."));

  replay(store, { frame: "delta", ...deltasFixture[3] });
  present(within(log).getByRole("button", { name: /Edit.*Update the bridge plan.*Pending/i }));
  replay(store, framesFixture[1]);
  const completed = within(log).getByRole("button", {
    name: /Edit.*Update the bridge plan.*Done/i,
  });
  fireEvent.click(completed);
  present(within(log).getByText("Updated doc/plans/bridge.md", { exact: true }));
  present(within(log).getByText("+Ready", { exact: true }));
}

async function restoreFile(chat: HTMLElement): Promise<void> {
  const footer = await within(chat).findByRole("group", { name: "Turn 1 changes" });
  present(within(footer).getByText("plans/bridge.md", { exact: true }));
  present(within(footer).getByText("Changed 1 files"));
  const diff = within(footer).getByRole("link", { name: "Diff" });
  const address = new URL(diff.getAttribute("href")!, "http://localhost");
  expect(address.pathname).toBe("/files/plans/bridge.md");
  expect(address.searchParams.get("diff")).toBe("turn");
  expect(address.searchParams.get("turn")).toBe(`${sessionId}/t1`);

  fireEvent.click(await within(footer).findByRole("button", { name: "Restore" }));
  await waitFor(() => {
    const posts = fetchMock.mock.calls.filter(
      ([input, init]) => pathOf(input) === "/api/restore" && init?.method === "POST",
    );
    expect(posts).toHaveLength(1);
    expect(JSON.parse(String(posts[0]![1]!.body))).toEqual({
      session: sessionId,
      turn: "t1",
      paths: ["doc/plans/bridge.md"],
      force: false,
    });
  });
  present(await within(footer).findByText(/Restored/));
}

beforeEach(() => {
  localStorage.clear();
  FakeSocket.last = undefined;
  replies = {
    "/api/server": hello().server,
    "/api/tree": treeFixture,
    "/api/concept/product": overviewFixture,
    "/api/hashes": { head: "c1", docs: {} },
    "/api/diff/turn": { path: "doc/plans/bridge.md", before: "Draft\n", after: "Ready\n" },
    "/api/restore": { files: [{ path: "doc/plans/bridge.md", result: "restored" }] },
  };
  fetchMock = vi.fn<typeof fetch>(answer);
  vi.stubGlobal("fetch", fetchMock);
  vi.spyOn(crypto, "randomUUID").mockReturnValue("e5122181-609e-44fd-9c24-85a8576b4fe5");
  vi.stubGlobal("matchMedia", (media: string) => ({
    ...matchMedia(media),
    matches: /min-width/.test(media),
  }));
});

afterEach(() => {
  cleanup();
  disposers.splice(0).forEach((dispose) => dispose());
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
});

it("chat replayed session sends attachments, shows the turn footer and restores a file", async () => {
  const store = await mount();
  const chat = await screen.findByRole("complementary", { name: "Chat" });
  await sendPassage(store, chat);
  await replayItems(store, chat);

  replay(store, { frame: "delta", ...deltasFixture[2] });
  // State fixtures describe independent sessions; retain this replay's session identity.
  const idle = deltasFixture.find(
    (delta) => delta.op === "session" && delta.session?.state === "idle",
  );
  const idleFrame: ChatFrame = chatFrameSchema.parse({
    frame: "delta",
    ...idle,
    session: { ...idle?.session, id: sessionId },
  });
  replay(store, idleFrame);
  await restoreFile(chat);
});
