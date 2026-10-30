import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { RouterProvider, createMemoryRouter } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import fixture from "@/plan/api/fixtures/chat-snapshot.json";
import hello from "@/plan/api/fixtures/hello.json";
import { checklistViewSchema, offerViewSchema } from "@/plan/api/schema";
import { chatSnapshotSchema, type SessionView } from "@/plan/api/schema-chat";
import { addAttachmentAtom, pendingAttachmentsAtom } from "@/plan/attachments";
import { OfferButton } from "@/plan/offers/offer-button";
import {
  authoringAtom,
  connectionAtom,
  projectAtom,
  serverInfoAtom,
  youAtom,
} from "@/plan/state/atoms";
import { endChat, startChat } from "./commands";
import { focusComposerAtom } from "./composer-hooks";
import { snapshotAtom } from "./model";
import { offerErrorAtom } from "./offer-error";
import { createOfferStarter } from "./offer-flow";
import { ChatPanel, ChatRegistrations } from "./panel";
import { panelCollapsedAtom } from "./prefs";
import { screenOfPath } from "./session-home";

vi.mock("./transcript", () => ({ Transcript: () => <div role="log">Transcript</div> }));
vi.mock("./composer", () => ({ ComposerArea: () => <div>Composer</div> }));
vi.mock("./attention", () => ({ useAttention: vi.fn() }));
vi.mock("./run-header", () => ({ RunHeader: () => <div>Run header</div> }));
vi.mock("./suggestion-cards", () => ({ useSuggestionFeed: vi.fn(), SuggestionCards: () => null }));
vi.mock("@/plan/review/live", async (original) => ({
  ...(await original<typeof import("@/plan/review/live")>()),
  useReviewLive: vi.fn(),
}));
vi.mock("./commands", async (original) => ({
  ...(await original<typeof import("./commands")>()),
  startChat: vi.fn().mockResolvedValue({ client_id: null, ok: true, error: null }),
  endChat: vi.fn(),
}));
vi.mock("./model", async (original) => ({
  ...(await original<typeof import("./model")>()),
  useChatSocket: vi.fn(),
}));

const { ComposerArea: RealComposerArea } =
  await vi.importActual<typeof import("./composer")>("./composer");
const offer = offerViewSchema.parse(hello.checklists[0]!.primary);
const checklist = checklistViewSchema.parse(hello.checklists[0]);
const base = chatSnapshotSchema.parse(fixture).session!;
const CONFIRM = /End the current chat first/;
let serial = 0;

function session(fields: Partial<SessionView> = {}): SessionView {
  return { ...base, state: "idle", feature: "alpha", ...fields };
}

function setup(path: string, current: SessionView | null) {
  const store = createStore();
  store.set(serverInfoAtom, { ...hello.server, project_key: `home-${++serial}`, read_only: [] });
  store.set(youAtom, hello.you);
  store.set(authoringAtom, { holder: { ...hello.you, away_since: null, frees_at: null } });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: Date.now() });
  store.set(projectAtom, {
    checklists: [
      { ...checklist, feature: "alpha", title: "Alpha Feature" },
      { ...checklist, feature: "beta", title: "Beta" },
    ],
    standalone: [{ slug: "solo", path: "doc/plans/PLAN-solo.md", state: "written", title: "Solo" }],
    moves: [],
  });
  store.set(snapshotAtom, { session: current, turns: [], queue: [], previous: null });
  const router = createMemoryRouter(
    [
      {
        path: "*",
        element: (
          <>
            <ChatRegistrations />
            <ChatPanel />
          </>
        ),
      },
    ],
    { initialEntries: [path] },
  );
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  // The offer mounts after the registration effect, as it does in the app.
  render(
    <Provider store={store}>
      <OfferButton offer={offer} />
    </Provider>,
  );
  return { store, router };
}

const panel = () => screen.getByRole("complementary", { name: "Chat" });
const notice = (title: string) =>
  within(panel()).queryByRole("link", { name: `A session in ${title} needs you` });
const clickOffer = async () => {
  fireEvent.click(screen.getByRole("button", { name: offer.label }));
  await act(async () => {});
};

beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
  vi.spyOn(window, "confirm").mockReturnValue(false);
  vi.spyOn(window, "matchMedia").mockImplementation((query) => ({
    matches: query.includes("min-width"),
    media: query,
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: () => true,
  }));
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it.each([
  ["/f/alpha", "alpha", null],
  ["/f/alpha/engineering/PLAN-x", "alpha", null],
  ["/files/product/alpha/section_01.md", "alpha", null],
  ["/files/plans/alpha/PLAN-x.md", "alpha", null],
  ["/files/plans/alpha/briefs/x/common.md", "alpha", null],
  ["/files/plans/PLAN-solo.md", null, "solo"],
  ["/files/plans/archive/DONE-PLAN-solo.md", null, "solo"],
  ["/files/plans/briefs/solo/x.md", null, "solo"],
  ["/files/plans/briefs/solo/round-1/x.md", null, "solo"],
  ["/files/plans/archive/briefs/old/x.md", null, "old"],
  ["/files/plans/briefs/x.md", null, null],
  ["/files/plans/archive/briefs/old.md", null, null],
  ["/files/plans/archive/misc/x.md", null, null],
  ["/plans/solo", null, "solo"],
  ["/files/product/misplaced.md", null, null],
  ["/files", null, null],
  ["/files/%E0%A4%A", null, null],
  ["/", null, null],
])("screenOfPath(%s) is owned by feature %s, plan %s", (path, feature, plan) => {
  expect(screenOfPath(path)).toEqual({ feature, plan });
});

it.each(["/f/alpha/product", "/f/alpha", "/files/product/alpha/section_01.md"])(
  "the session shows on %s, its own screen",
  (path) => {
    setup(path, session());
    expect(within(panel()).getByRole("log")).toBeTruthy();
    expect(panel().querySelector(".chat-state-pill")?.textContent).toBe("Waiting for you");
    expect(notice("Alpha Feature")).toBeNull();
  },
);

it.each(["/f/beta/product", "/files/product/beta/section_01.md", "/plans/solo", "/files"])(
  "on %s the panel is the empty state with a link to the session",
  (path) => {
    setup(path, session());
    expect(within(panel()).queryByRole("log")).toBeNull();
    expect(within(panel()).queryByText("Composer")).toBeNull();
    expect(within(panel()).getByRole("heading", { name: "Chat" })).toBeTruthy();
    expect(within(panel()).getByRole("button", { name: "New chat" })).toBeTruthy();
    expect(notice("Alpha Feature")?.getAttribute("href")).toBe("/f/alpha");
  },
);

it("navigating between a session's feature and another moves it in and out of the panel", async () => {
  const { router } = setup("/f/alpha/product", session());
  expect(within(panel()).getByRole("log")).toBeTruthy();
  await act(() => router.navigate("/f/beta/product"));
  expect(within(panel()).queryByRole("log")).toBeNull();
  expect(notice("Alpha Feature")).toBeTruthy();
  await act(() => router.navigate("/f/alpha/product"));
  expect(within(panel()).getByRole("log")).toBeTruthy();
});

it("a concluded session from another feature leaves a plain empty state, with no notice", () => {
  setup("/f/beta/product", session({ state: "ended" }));
  expect(within(panel()).queryByRole("log")).toBeNull();
  expect(within(panel()).queryByRole("link")).toBeNull();
  expect(within(panel()).getByRole("button", { name: "New chat" })).toBeTruthy();
});

it("a chat started for no feature belongs to the project and shows on every screen", () => {
  setup("/f/beta/product", session({ feature: null }));
  expect(within(panel()).getByRole("log")).toBeTruthy();
});

it("a pressure test of a standalone plan belongs to that plan", () => {
  const pressure = session({ kind: "pressure", feature: null, plan: "solo" });
  setup("/plans/solo", pressure);
  expect(within(panel()).getByRole("log")).toBeTruthy();
  cleanup();
  setup("/files/plans/briefs/solo/common.md", pressure);
  expect(within(panel()).getByRole("log")).toBeTruthy();
  cleanup();
  setup("/f/beta/product", pressure);
  expect(within(panel()).queryByRole("log")).toBeNull();
  expect(notice("Solo")?.getAttribute("href")).toBe("/plans/solo");
});

it("the empty state of another session's screen offers no End chat", () => {
  setup("/f/beta/product", session());
  expect(within(panel()).queryByRole("button", { name: "End chat" })).toBeNull();
  expect(within(panel()).queryByLabelText("Chat menu")).toBeNull();
  cleanup();
  setup("/f/alpha/product", session());
  expect(within(panel()).getByRole("button", { name: "End chat" })).toBeTruthy();
});

it("the c key does nothing while the panel has no composer to focus", () => {
  const away = setup("/f/beta/product", session());
  act(() => away.store.set(panelCollapsedAtom, true));
  fireEvent.keyDown(document.body, { key: "c" });
  expect(away.store.get(focusComposerAtom)).toBe(false);
  expect(away.store.get(panelCollapsedAtom)).toBe(true);
  cleanup();
  const home = setup("/f/alpha/product", session());
  fireEvent.keyDown(document.body, { key: "c" });
  expect(home.store.get(focusComposerAtom)).toBe(true);
});

it("an offer with the open session on another screen raises no confirm and says why", async () => {
  const { store } = setup("/f/beta/product", session());
  await clickOffer();
  expect(window.confirm).not.toHaveBeenCalled();
  expect(endChat).not.toHaveBeenCalled();
  expect(startChat).not.toHaveBeenCalled();
  expect(within(panel()).getByRole("alert").textContent).toContain(
    "Finish the session in Alpha Feature first.",
  );
  expect(store.get(offerErrorAtom)).not.toBeNull();
});

it("New chat with the open session on another screen raises no confirm", async () => {
  setup("/f/beta/product", session());
  fireEvent.click(within(panel()).getByRole("button", { name: "New chat" }));
  await act(async () => {});
  expect(window.confirm).not.toHaveBeenCalled();
  expect(endChat).not.toHaveBeenCalled();
  expect(startChat).not.toHaveBeenCalled();
  expect(within(panel()).getByRole("alert").textContent).toContain("Alpha Feature");
});

it("with nothing open anywhere an offer raises no confirm and starts", async () => {
  const { store } = setup("/f/beta/product", null);
  await clickOffer();
  expect(window.confirm).not.toHaveBeenCalled();
  expect(startChat).toHaveBeenCalledWith(
    store,
    offer.id,
    undefined,
    undefined,
    offer.feature,
    undefined,
  );
});

it("Cancel on this screen's own session ends the matter: no second confirm, even on navigating", async () => {
  const { router, store } = setup("/f/alpha/product", session());
  await clickOffer();
  expect(window.confirm).toHaveBeenCalledTimes(1);
  expect(vi.mocked(window.confirm).mock.calls[0]?.[0]).toMatch(CONFIRM);
  expect(endChat).not.toHaveBeenCalled();
  await act(async () => {});
  await act(() => router.navigate("/f/alpha/engineering"));
  await act(() => router.navigate("/f/alpha/product"));
  expect(window.confirm).toHaveBeenCalledTimes(1);
  expect(startChat).not.toHaveBeenCalled();
  expect(store.get(offerErrorAtom)).toBeNull();
});

it("a refusal goes with the screen it was raised on", async () => {
  const { router, store } = setup("/f/beta/product", session());
  await clickOffer();
  expect(store.get(offerErrorAtom)).not.toBeNull();
  await act(() => router.navigate("/f/alpha/product"));
  expect(store.get(offerErrorAtom)).toBeNull();
});

it("New chat on a feature's screen starts the chat for that feature", async () => {
  const { store } = setup("/f/beta/product", null);
  fireEvent.click(within(panel()).getByRole("button", { name: "New chat" }));
  await act(async () => {});
  expect(startChat).toHaveBeenCalledWith(store, null, null, [], "beta", undefined);
});

it.each([
  ["/plans/solo", undefined, "solo"],
  ["/files/plans/briefs/solo/common.md", undefined, "solo"],
  // Names no feature or plan exists for: the server would refuse them as a start's owner.
  ["/f/ghost", undefined, undefined],
  ["/plans/ghost", undefined, undefined],
  ["/files/plans/briefs/ghost/common.md", undefined, undefined],
])("New chat on %s starts the chat for feature %s, plan %s", async (path, feature, plan) => {
  const { store } = setup(path, null);
  fireEvent.click(within(panel()).getByRole("button", { name: "New chat" }));
  await act(async () => {});
  expect(startChat).toHaveBeenCalledWith(store, null, null, [], feature, plan);
});

it("an offer that names no feature, chosen on a plan's screen, starts for that plan", async () => {
  const { store } = setup("/plans/solo", null);
  await createOfferStarter(store).start("refine:doc/plans/PLAN-solo.md");
  expect(startChat).toHaveBeenCalledWith(
    store,
    "refine:doc/plans/PLAN-solo.md",
    undefined,
    undefined,
    undefined,
    "solo",
  );
});

it.each([
  ["pressure", "loom plan: pressure", "Pressure test"],
  ["authoring", "loom plan: chat", "Chat"],
  ["authoring", "recovered authoring session", "Chat"],
  ["authoring", "Planning session", "Planning session"],
])("with no brief a %s session named %s is titled %s", async (kind, name, title) => {
  const fetchMock = vi.fn(
    async () => new Response(JSON.stringify({ error: "unknown_session" }), { status: 404 }),
  );
  vi.stubGlobal("fetch", fetchMock);
  setup("/f/alpha/product", session({ kind, name }));
  await waitFor(() => expect(fetchMock).toHaveBeenCalled());
  await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
  expect(within(panel()).getByRole("heading", { name: title })).toBeTruthy();
});

it("the head reads Chat with New chat when empty, and the state pill with a session", () => {
  setup("/f/beta/product", null);
  const head = () => within(panel()).getAllByRole("banner", { hidden: true })[0]!;
  expect(within(head()).getByRole("heading", { name: "Chat" })).toBeTruthy();
  expect(within(head()).getByRole("button", { name: "New chat" })).toBeTruthy();
  cleanup();
  setup("/f/alpha/product", session({ state: "permission" }));
  const pill = within(head()).getByText("Needs a decision");
  expect(pill.getAttribute("data-state")).toBe("permission");
  // Beside a running chat New chat moves into the menu; the pill holds its place.
  const newChat = within(head()).getByRole("button", { name: "New chat" });
  expect(newChat.closest("details")).not.toBeNull();
});

const draft = {
  kind: "source" as const,
  path: "doc/product/beta/x.md",
  anchor: null,
  lines: null,
  stageId: null,
  text: "x",
  hash: null,
};

it("a pending attachment shows its chip on a screen with no open chat, not beside an open foreign one", () => {
  const ended = setup("/f/beta/product", session({ state: "ended" }));
  act(() => ended.store.set(addAttachmentAtom, draft));
  expect(within(panel()).getByLabelText("Pending attachments")).toBeTruthy();
  cleanup();
  const open = setup("/f/beta/product", session());
  act(() => open.store.set(pendingAttachmentsAtom, [draft]));
  expect(within(panel()).queryByLabelText("Pending attachments")).toBeNull();
});

it("a concluded session on its own screen shows the pending chip beside its offers", () => {
  const { store } = setup("/f/alpha/product", session({ state: "ended" }));
  render(
    <Provider store={store}>
      <RouterProvider router={createMemoryRouter([{ path: "*", element: <RealComposerArea /> }])} />
    </Provider>,
  );
  expect(screen.queryByLabelText("Pending attachments")).toBeNull();
  act(() => store.set(addAttachmentAtom, draft));
  expect(screen.getByLabelText("Pending attachments")).toBeTruthy();
});

it("a browser that cannot author sees no chip on a concluded foreign screen", () => {
  const { store } = setup("/f/beta/product", session({ state: "ended" }));
  const other = { ...hello.you, public_id: "fedcba9876543210", name: "Someone Else" };
  act(() => {
    store.set(authoringAtom, { holder: { ...other, away_since: null, frees_at: null } });
    store.set(pendingAttachmentsAtom, [draft]);
  });
  expect(within(panel()).queryByLabelText("Pending attachments")).toBeNull();
});
