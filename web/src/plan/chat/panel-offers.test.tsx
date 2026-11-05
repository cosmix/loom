import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import fixture from "@/plan/api/fixtures/chat-snapshot.json";
import hello from "@/plan/api/fixtures/hello.json";
import { offerViewSchema } from "@/plan/api/schema";
import { chatSnapshotSchema } from "@/plan/api/schema-chat";
import { OfferButton } from "@/plan/offers/offer-button";
import { authoringAtom, connectionAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";
import { endChat, startChat } from "./commands";
import { snapshotAtom } from "./model";
import { offerErrorAtom } from "./offer-error";
import { DESKTOP } from "./offer-flow";
import { ChatPanel, ChatRegistrations } from "./panel";
import { chatSheetOpenAtom } from "./sheet-state";

vi.mock("./transcript", () => ({ Transcript: () => <div role="log">Transcript</div> }));
vi.mock("./composer", () => ({ ComposerArea: () => <div>Composer</div> }));
vi.mock("./attention", () => ({ useAttention: vi.fn() }));
vi.mock("./run-header", () => ({ RunHeader: () => <div>Run header</div> }));
vi.mock("./start-chat", () => ({ NewChatButton: () => null, StartingBrief: () => null }));
vi.mock("./end-chat", () => ({ EndChatMenuItem: () => null, useCanEndChat: () => false }));
vi.mock("./suggestion-cards", () => ({ useSuggestionFeed: vi.fn() }));
vi.mock("@/plan/review/live", () => ({ useReviewLive: vi.fn() }));
vi.mock("@/plan/review/changed-since", () => ({ ChangedSinceControl: () => null }));
vi.mock("./commands", async (original) => ({
  ...(await original<typeof import("./commands")>()),
  startChat: vi.fn(),
  endChat: vi.fn(),
}));
vi.mock("./model", async (original) => ({
  ...(await original<typeof import("./model")>()),
  useChatSocket: vi.fn(),
}));

const offerView = offerViewSchema.parse(hello.checklists[0]!.primary);
const running = { ...chatSnapshotSchema.parse(fixture).session!, state: "idle" as const };
const CONFIRM =
  "End the current chat first? It will write its open questions into the documents before it stops.";
let serial = 0;

function media(desktop: boolean) {
  vi.spyOn(window, "matchMedia").mockImplementation((query) => ({
    matches: query === DESKTOP ? desktop : false,
    media: query,
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: () => true,
  }));
}

function mount(session = running as typeof running | null, selectedOffer = offerView) {
  const store = createStore();
  store.set(serverInfoAtom, { ...hello.server, project_key: `offers-${++serial}`, read_only: [] });
  store.set(youAtom, hello.you);
  store.set(authoringAtom, { holder: { ...hello.you, away_since: null, frees_at: null } });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: Date.now() });
  store.set(snapshotAtom, { session, turns: [], queue: [], previous: null });
  render(
    <Provider store={store}>
      <ChatRegistrations />
      <ChatPanel />
    </Provider>,
  );
  // The offer mounts after the shell's registration effect, as it does in the app.
  render(
    <Provider store={store}>
      <OfferButton offer={selectedOffer} />
    </Provider>,
  );
  return store;
}

const clickOffer = async () => {
  fireEvent.click(screen.getByRole("button", { name: offerView.label }));
  await act(async () => {});
};

beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
  vi.spyOn(window, "confirm").mockReturnValue(true);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it.each([
  ["the side panel, which is always shown,", true],
  ["a closed sheet below the desktop width", false],
])("a refused offer start shows its alert in %s", async (_name, desktop) => {
  media(desktop);
  vi.mocked(startChat).mockRejectedValueOnce(new Error("Chat failed to start"));
  const store = mount(null);
  expect(screen.queryByRole("alert")).toBeNull();
  await clickOffer();
  expect(screen.getByRole("alert").textContent).toContain("Chat failed to start");
  // Only the sheet needs opening; the side panel is already there.
  expect(store.get(chatSheetOpenAtom)).toBe(!desktop);
});

it("an offer start disposed while in flight writes no error and reveals nothing", async () => {
  media(false);
  let fail: (error: Error) => void = () => {};
  vi.mocked(startChat).mockImplementationOnce(
    () => new Promise((_resolve, reject) => (fail = reject)),
  );
  const store = mount(null);
  await clickOffer();
  cleanup();
  fail(new Error("Chat failed to start"));
  await act(async () => {});
  expect(store.get(offerErrorAtom)).toBeNull();
  expect(store.get(chatSheetOpenAtom)).toBe(false);
});

it("an offer with a chat running asks, ends it, waits, then starts", async () => {
  media(true);
  const store = mount();
  vi.mocked(endChat).mockImplementation(async () => ({ client_id: null, ok: true, error: null }));
  await clickOffer();
  expect(window.confirm).toHaveBeenCalledWith(CONFIRM);
  expect(endChat).toHaveBeenCalledTimes(1);
  expect(startChat).not.toHaveBeenCalled();
  act(() =>
    store.set(snapshotAtom, {
      session: { ...running, state: "ended" },
      turns: [],
      queue: [],
      previous: null,
    }),
  );
  await act(async () => {});
  expect(startChat).toHaveBeenCalledWith(
    store,
    offerView.id,
    undefined,
    undefined,
    offerView.feature,
    undefined,
  );
});

it.each([
  ["draft_all_sections", "draft-all-sections"],
  ["write_section", "write-section:02"],
] as const)("the panel forwards the feature for %s", async (kind, id) => {
  media(true);
  const store = mount(null, { ...offerView, kind, id, feature: "demo" });

  await clickOffer();

  expect(startChat).toHaveBeenCalledExactlyOnceWith(
    store,
    id,
    undefined,
    undefined,
    "demo",
    undefined,
  );
});

it("declining the end-first question changes nothing", async () => {
  media(true);
  vi.mocked(window.confirm).mockReturnValue(false);
  const store = mount();
  await clickOffer();
  expect(endChat).not.toHaveBeenCalled();
  expect(startChat).not.toHaveBeenCalled();
  expect(store.get(offerErrorAtom)).toBeNull();
});

it("losing the start race after ending names the holder in the panel", async () => {
  media(true);
  vi.mocked(startChat).mockRejectedValueOnce(new Error("Another browser holds authoring (Alex)"));
  const store = mount();
  await clickOffer();
  act(() =>
    store.set(snapshotAtom, {
      session: { ...running, state: "ended", holder: "Alex" },
      turns: [],
      queue: [],
      previous: null,
    }),
  );
  await act(async () => {});
  const panel = screen.getByRole("complementary", { name: "Chat" });
  expect(within(panel).getByRole("alert").textContent).toContain("Alex started a chat first");
});
