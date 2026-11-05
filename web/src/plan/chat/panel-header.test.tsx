import { render, screen, within } from "@testing-library/react";
import { Provider } from "jotai";
import { expect, it, vi } from "vitest";

import fixture from "@/plan/api/fixtures/chat-snapshot.json";
import hello from "@/plan/api/fixtures/hello.json";
import { checklistViewSchema } from "@/plan/api/schema";
import { chatSnapshotSchema, type SessionView } from "@/plan/api/schema-chat";
import { checklistAtom } from "@/plan/state/atoms";
import { snapshotAtom } from "./model";
import {
  installPanelLifecycle,
  media,
  mount,
  permissionSnapshot,
  storeFor,
} from "./panel-test-kit";
import { RunHeader } from "./run-header";

vi.mock("./transcript", () => ({ Transcript: () => <div role="log">Transcript</div> }));
vi.mock("./composer", () => ({ ComposerArea: () => <div>Composer</div> }));
vi.mock("./attention", () => ({ useAttention: vi.fn() }));
vi.mock("./start-chat", () => ({
  NewChatButton: () => <button>New chat</button>,
  StartingBrief: () => <div>Starting brief</div>,
}));
vi.mock("./end-chat", () => ({ useCanEndChat: () => false }));
vi.mock("./suggestion-cards", async (original) => ({
  ...(await original<typeof import("./suggestion-cards")>()),
  useSuggestionFeed: vi.fn(),
}));
vi.mock("@/plan/review/live", () => ({ useReviewLive: vi.fn() }));
vi.mock("./model", async (original) => ({
  ...(await original<typeof import("./model")>()),
  useChatSocket: vi.fn(),
}));

installPanelLifecycle();

it.each([true, false])(
  "the empty row holds the label and no offer (checklist primary: %s)",
  (primary) => {
    const store = storeFor();
    const checklist = checklistViewSchema.parse(hello.checklists[0]);
    const title = "Pressure-test adopt-ecc-…";
    store.set(checklistAtom, {
      ...checklist,
      primary: primary ? { ...checklist.primary!, label: title } : null,
    });
    const { container } = mount(store);
    const line = container.querySelector<HTMLElement>(".chat-state-line")!;
    // With no chat the line draws no visible words, only the polite region's "No chat".
    expect(line.querySelector(".chat-state-row")).toBeNull();
    expect(within(line).getByRole("status").textContent).toBe("No chat");
    expect(within(container).queryByText(title)).toBeNull();
    const head = container.querySelector("header")!;
    const heading = within(head).getByRole("heading", { name: "New chat" });
    expect(heading.hasAttribute("data-new")).toBe(true);
  },
);

it.each(["starting", "working", "idle", "permission", "ending", "ended", "failed"] as const)(
  "the run line draws nothing for an authoring chat while %s",
  (state) => {
    const store = storeFor();
    const snapshot = chatSnapshotSchema.parse(fixture);
    const session = { ...snapshot.session!, kind: "authoring" as const, state };
    store.set(snapshotAtom, { ...snapshot, session });
    const { container } = render(
      <Provider store={store}>
        <RunHeader session={session} />
      </Provider>,
    );
    expect(container.textContent).toBe("");
    expect(container.querySelector(".chat-run-header")).toBeNull();
  },
);

const subLines: Array<[SessionView["state"], string | null]> = [
  ["starting", "Starting"],
  ["working", "Working"],
  ["idle", null],
  ["ending", "Wrapping up"],
  ["ended", "Ended"],
  ["failed", "Stopped"],
];

it.each(subLines)("the head's sub-line for %s shows %s", (state, words) => {
  const store = storeFor();
  const snapshot = chatSnapshotSchema.parse(fixture);
  store.set(snapshotAtom, {
    ...snapshot,
    session: { ...snapshot.session!, kind: "authoring", state, failure: null },
    previous: null,
  });
  const { container } = mount(store);
  const sub = container.querySelector<HTMLElement>("header .chat-head-sub .chat-state-row");
  expect(sub?.textContent ?? null).toBe(words);
});

it("the head's sub-line shows Needs your answer for a permission state", () => {
  const store = storeFor();
  store.set(snapshotAtom, permissionSnapshot());
  const { container } = mount(store);
  const row = container.querySelector<HTMLElement>("header .chat-head-sub .chat-state-row")!;
  expect(row.textContent).toContain("Needs your answer");
  expect(row.querySelector("[aria-hidden=true]")?.textContent).toBe("⚠");
  expect(container.querySelectorAll(".aurora-roundel")).toHaveLength(0);
});

it("the sheet button's permission glyph keeps its role and accessible name", () => {
  media(false);
  const store = storeFor();
  store.set(snapshotAtom, permissionSnapshot());
  mount(store);
  const button = screen.getByRole("button", { name: "Open chat, 1 decision waiting" });
  const glyph = within(button).getByRole("img", { name: "Needs a decision" });
  expect(glyph.querySelector('[data-kind="permission"]')).toBeTruthy();
});

it.each([
  ["starting", "Starting session", "starting"],
  ["working", "Working", "drafting"],
  ["ending", "Writing final notes", "drafting"],
] as const)("the sheet button's %s glyph keeps its status text", (state, label, kind) => {
  media(false);
  const store = storeFor();
  const snapshot = chatSnapshotSchema.parse(fixture);
  store.set(snapshotAtom, {
    ...snapshot,
    session: { ...snapshot.session!, state },
    turns: [],
  });
  mount(store);
  const button = screen.getByRole("button", { name: `Open chat, ${label}` });
  expect(within(button).getByRole("status").textContent).toBe(label);
  expect(button.querySelector(`.activity-glyph[data-kind="${kind}"]`)).toBeTruthy();
});
