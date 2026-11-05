import { render, screen, within } from "@testing-library/react";
import { Provider } from "jotai";
import { expect, it, vi } from "vitest";

import fixture from "@/plan/api/fixtures/chat-snapshot.json";
import hello from "@/plan/api/fixtures/hello.json";
import { checklistViewSchema } from "@/plan/api/schema";
import { chatSnapshotSchema, type SessionView } from "@/plan/api/schema-chat";
import { checklistAtom, presenceAtom } from "@/plan/state/atoms";
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
vi.mock("./end-chat", () => ({ EndChatMenuItem: () => null, useCanEndChat: () => false }));
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
    const row = line.querySelector<HTMLElement>('[aria-live="off"] > .flex')!;
    const label = within(row.children[1] as HTMLElement).getByText("No chat");
    const hint = within(row).queryByText(title);
    expect(row.children[0].getAttribute("aria-hidden")).toBe("true");
    expect(row.children[1].contains(label)).toBe(true);
    expect(row.children[1].contains(hint)).toBe(false);
    // The panel offers New chat and prompts; the workflow offer belongs to the page.
    expect(row.children).toHaveLength(2);
    // With no chat the head offers New chat, so the row does not repeat it.
    expect(within(row).queryByText("New chat")).toBeNull();
    expect(within(container.querySelector("header")!).getByText("New chat")).toBeTruthy();
    expect(within(line).getByRole("status").textContent).toBe("No chat");
  },
);

const headerStates: Array<[SessionView["state"], string | null]> = [
  ["starting", "led"],
  ["working", "led"],
  ["ending", "led"],
  ["idle", "waiting"],
  ["permission", null],
  ["ended", null],
  ["failed", null],
];

it.each(headerStates)("run header uses the expected glyph while %s", (state, kind) => {
  const store = storeFor();
  const snapshot = chatSnapshotSchema.parse(fixture);
  const session = { ...snapshot.session!, kind: "section", state };
  store.set(snapshotAtom, {
    ...snapshot,
    session,
    turns: [
      {
        ...snapshot.turns[0],
        tokens: { input: 1000, cache_write: 200, cache_read: 9999, output: 300 },
      },
    ],
  });
  store.set(presenceAtom, {
    entries: [
      { public_id: "watcher", name: "Alex", holder: false, away: false, reading: null, tabs: 1 },
    ],
  });
  const { container } = render(
    <Provider store={store}>
      <RunHeader session={session} />
    </Provider>,
  );
  const header = container.querySelector<HTMLElement>(".chat-run-header")!;
  const glyph = header.querySelector(".activity-glyph");
  if (kind === "led") {
    expect(header.firstElementChild?.classList.contains("chat-led")).toBe(true);
    expect(header.firstElementChild?.getAttribute("aria-hidden")).toBe("true");
    expect(glyph).toBeNull();
  } else if (kind) {
    expect(glyph?.getAttribute("data-kind")).toBe(kind);
    expect(glyph?.getAttribute("aria-hidden")).toBe("true");
    expect(header.firstElementChild).toBe(glyph);
  } else {
    expect(glyph).toBeNull();
  }
  expect(header.textContent).toContain(`${hello.you.name} authoring`);
  expect(screen.getByLabelText("1 watching: Alex").getAttribute("title")).toBe("Alex");
  expect(screen.getByText("· 1.5k tokens")).toBeTruthy();
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
