import { cleanup, render, screen, within } from "@testing-library/react";
import { Provider } from "jotai";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router";
import { beforeEach, expect, it, vi } from "vitest";

import type { ChatSnapshot, Item, Turn } from "@/plan/api/schema-chat";
import { snapshotAtom } from "./model";
import { ChatPanel, ChatRegistrations } from "./panel";
import { PanelHead } from "./panel-head";
import { installPanelLifecycle, storeFor } from "./panel-test-kit";
import { StateLine } from "./state-line";
import { fixture, item, mount, turn } from "./transcript-test-kit";

vi.mock("./attention", () => ({ useAttention: vi.fn() }));
vi.mock("./suggestion-cards", async (original) => ({
  ...(await original<typeof import("./suggestion-cards")>()),
  useSuggestionFeed: vi.fn(),
}));
vi.mock("@/plan/review/live", async (original) => ({
  ...(await original<typeof import("@/plan/review/live")>()),
  useReviewLive: vi.fn(),
}));
vi.mock("./model", async (original) => ({
  ...(await original<typeof import("./model")>()),
  useChatSocket: vi.fn(),
}));

// Wide matchMedia, cleanup, mock restore and global unstub per test.
installPanelLifecycle();

beforeEach(() => {
  // The head and the opening label load the session brief; it never answers here.
  vi.stubGlobal(
    "fetch",
    vi.fn(() => new Promise<Response>(() => {})),
  );
});

type Session = NonNullable<ChatSnapshot["session"]>;

/** A wide desktop: every min-width breakpoint holds, whatever width the panel switches at. */
function wideViewport() {
  vi.spyOn(window, "matchMedia").mockImplementation((query) => ({
    matches: /min-width/.test(query) && !/max-width|prefers-reduced-motion/.test(query),
    media: query,
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: () => true,
  }));
}

function session(state: Session["state"]): Session {
  const fields = { state, kind: "authoring" as const, failure: null, feature: null, plan: null };
  return { ...fixture.session!, ...fields };
}

function storeWith(state: Session["state"], turns: Turn[] = []) {
  const store = storeFor();
  store.set(snapshotAtom, { session: session(state), turns, queue: [], previous: null });
  return store;
}

function renderIn(store: ReturnType<typeof storeFor>, children: ReactNode) {
  return render(
    <Provider store={store}>
      <MemoryRouter>{children}</MemoryRouter>
    </Provider>,
  );
}

const userItem = (id: string, text: string): Item =>
  item("user", { id, text, origin: null, attachments: [], delivery: "delivered" });
const read = (id: string, status: "pending" | "done" = "pending"): Item =>
  item("tool", {
    id,
    tool_use_id: `tu-${id}`,
    name: "Read",
    status,
    summary: "Read doc/plans/bridge.md",
  });

it("a live chat head has no menu and no End chat", () => {
  const { container } = renderIn(storeWith("idle"), <PanelHead sheet={false} />);
  expect(screen.queryByLabelText("Chat menu")).toBeNull();
  expect(container.querySelector("details")).toBeNull();
  expect(screen.queryByRole("button", { name: "End chat", hidden: true })).toBeNull();
  expect(screen.getByRole("button", { name: "New chat" })).toBeTruthy();
});

it("the state line draws nothing for an idle chat", () => {
  const idle = renderIn(storeWith("idle"), <StateLine />);
  const line = idle.container.querySelector(".chat-state-line");
  expect((line?.textContent ?? "").trim()).toBe("");
  expect(idle.container.textContent).not.toContain("Waiting for you");
  cleanup();

  const open = turn([userItem("u1", "Look at the bridge plan."), read("r1")]);
  const working = renderIn(storeWith("working", [open]), <StateLine />);
  expect(working.container.textContent).toContain("Working");
});

const CLOSING_PROMPT = [
  "loom plan is ending this session; nobody will answer further questions.",
  "1. Write into the documents every decision this conversation settled that is not in them yet.",
].join("\n");
const CLOSING_BLOCK = JSON.stringify({
  decisions: [
    "A chat ends when a new chat starts or another task opens.",
    "<b>Bold</b> stays text.",
  ],
  questions: ["Should an ended chat fold to its card?"],
});
const SECTION_04 = "doc/product/demo/section_04.md";

/** The ended chat: one talk turn, then the closing turn with its prompt, an edit and the reply. */
function endedChat(): NonNullable<ChatSnapshot["previous"]> {
  const talk = turn(
    [
      userItem("u1", "Settle how a chat ends."),
      item("text", { id: "a1", markdown: "Noted, the removal stays a step in the plan." }),
    ],
    { id: "t1", open: false, started_at: "2026-11-05T10:00:00Z", changed_files: [] },
  );
  const prompt = item("user", {
    id: "u2",
    text: CLOSING_PROMPT,
    origin: "closing",
    attachments: [],
    delivery: "delivered",
  });
  const edit = item("tool", {
    id: "a2-tool",
    tool_use_id: "tu2",
    name: "Edit",
    status: "done",
    summary: SECTION_04,
    input: JSON.stringify({ file_path: SECTION_04 }),
    added: 9,
    removed: 2,
  });
  const markdown = `Closing write done.\n\n\`\`\`loom-closing\n${CLOSING_BLOCK}\n\`\`\``;
  const reply = item("text", { id: "a2", markdown });
  const changed = [{ path: SECTION_04, before_hash: "h1", by_tool: true }];
  const closing = turn([prompt, edit, reply], {
    id: "t2",
    open: false,
    started_at: "2026-11-05T10:05:00Z",
    changed_files: changed,
  });
  return { session: session("ended"), turns: [talk, closing] };
}

it("an ended chat shows what it left behind and never the closing prompt", () => {
  const { log } = mount([], { session: null, turns: [], previous: endedChat() });
  const shown = log.textContent ?? "";
  expect(shown).toContain("Settle how a chat ends.");
  expect(shown).toContain("Noted, the removal stays a step in the plan.");
  expect(shown).toMatch(/what this chat left behind/);
  expect(shown).toContain("A chat ends when a new chat starts or another task opens.");
  expect(shown).toContain("<b>Bold</b> stays text.");
  expect(shown).toContain("Should an ended chat fold to its card?");
  expect(shown).toContain("section_04.md");
  for (const hidden of [
    "loom plan is ending this session",
    "Closing message from loom plan",
    "Closing write done",
    "loom-closing",
    "fresh tokens",
  ]) {
    expect(shown).not.toContain(hidden);
  }
  expect(log.querySelector("b")).toBeNull();
  expect(within(log).queryByRole("button", { name: "Show" })).toBeNull();
});

/**
 * Ten closed turns, each a question, a reply and a done edit; then `t11` with a Read, open and
 * pending when `working`, closed and done otherwise (a finished turn holds no pending tool).
 */
function elevenTurns(working: boolean): Turn[] {
  const closed = Array.from({ length: 10 }, (_, index) => {
    const n = index + 1;
    const edit = item("tool", {
      id: `e${n}`,
      tool_use_id: `tu-e${n}`,
      name: "Edit",
      status: "done",
      summary: `doc/plans/p${n}.md`,
      added: 1,
      removed: 1,
    });
    const items = [
      userItem(`u${n}`, `Question ${n}`),
      item("text", { id: `a${n}`, markdown: `Answer ${n}` }),
      edit,
    ];
    const started_at = `2026-11-05T10:${String(n).padStart(2, "0")}:00Z`;
    return turn(items, { id: `t${n}`, open: false, started_at, changed_files: [] });
  });
  const last = turn([userItem("u11", "Question 11"), read("r11", working ? "pending" : "done")], {
    id: "t11",
    open: working,
    started_at: "2026-11-05T10:11:00Z",
    changed_files: [],
  });
  return [...closed, last];
}

/** The typing bubbles and roundels the chat pane draws in `state`. */
function signsIn(state: Session["state"], turns: Turn[]): { typing: number; roundels: number } {
  const store = storeWith(state, turns);
  renderIn(
    store,
    <>
      <ChatRegistrations />
      <ChatPanel />
    </>,
  );
  const chat = screen.getByRole("complementary", { name: "Chat" });
  return {
    typing: chat.querySelectorAll(".chat-typing").length,
    roundels: chat.querySelectorAll(".aurora-roundel").length,
  };
}

it("a working chat draws one typing bubble and no roundel, an idle chat neither", () => {
  wideViewport();
  expect(signsIn("working", elevenTurns(true))).toEqual({ typing: 1, roundels: 0 });
  cleanup();
  expect(signsIn("idle", elevenTurns(false))).toEqual({ typing: 0, roundels: 0 });
});
