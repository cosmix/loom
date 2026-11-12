import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Provider } from "jotai";
import { RouterProvider, createMemoryRouter } from "react-router";
import { expect, it, vi } from "vitest";

import hello from "@/plan/api/fixtures/hello.json";
import { offerViewSchema } from "@/plan/api/schema";
import { addAttachmentAtom, pendingAttachmentsAtom } from "@/plan/attachments";
import { activeFeatureAtom, authoringAtom } from "@/plan/state/atoms";
import { endChat, startChat } from "./commands";
import { focusComposerAtom } from "./composer-hooks";
import { snapshotAtom } from "./model";
import { offerErrorAtom } from "./offer-error";
import { createOfferStarter } from "./offer-flow";
import { screenOfPath, startOwnerAtom } from "./session-home";
import {
  clickOffer,
  installHomeLifecycle,
  notice,
  offer,
  panel,
  session,
  setup,
} from "./session-home-kit";
import { chatSheetOpenAtom } from "./sheet-state";

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
const CONFIRM = /End the current chat first/;

installHomeLifecycle();

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
    // An idle chat draws no state words: the composer is the cue.
    expect(panel().querySelector(".chat-state-row")).toBeNull();
    expect(panel().textContent).not.toContain("Waiting for you");
    expect(notice("alpha")).toBeNull();
  },
);

it.each(["/f/beta/product", "/files/product/beta/section_01.md", "/plans/solo", "/files"])(
  "on %s the panel is the empty state with a link to the session",
  (path) => {
    setup(path, session());
    expect(within(panel()).queryByRole("log")).toBeNull();
    expect(within(panel()).queryByText("Composer")).toBeNull();
    expect(within(panel()).getByRole("heading", { name: "New chat" })).toBeTruthy();
    // A chat that belongs to another screen reads as no chat here: nothing to end, so no New chat.
    expect(within(panel()).queryByRole("button", { name: "New chat" })).toBeNull();
    expect(notice("alpha")?.getAttribute("href")).toBe("/f/alpha");
  },
);

it("navigating between a session's feature and another moves it in and out of the panel", async () => {
  const { router } = setup("/f/alpha/product", session());
  expect(within(panel()).getByRole("log")).toBeTruthy();
  await act(() => router.navigate("/f/beta/product"));
  expect(within(panel()).queryByRole("log")).toBeNull();
  expect(notice("alpha")).toBeTruthy();
  await act(() => router.navigate("/f/alpha/product"));
  expect(within(panel()).getByRole("log")).toBeTruthy();
});

it.each([
  ["another feature", session({ state: "ended" })],
  [
    "another plan's pressure test",
    session({ kind: "pressure", feature: null, plan: "solo", state: "ended" }),
  ],
])("a concluded session from %s leaves a fresh chat with a composer and no notice", (_, ended) => {
  setup("/f/beta/product", ended);
  expect(within(panel()).queryByRole("log")).toBeNull();
  expect(within(panel()).queryByRole("link")).toBeNull();
  expect(within(panel()).getByText("Composer")).toBeTruthy();
});

it("a pressure test concluded into `previous` leaves another plan's screen a composer", () => {
  const { store } = setup("/f/beta/product", null);
  const ended = session({ kind: "pressure", feature: null, plan: "solo", state: "ended" });
  act(() =>
    store.set(snapshotAtom, {
      session: null,
      turns: [],
      queue: [],
      previous: { session: ended, turns: [] },
    }),
  );
  expect(within(panel()).getByText("Composer")).toBeTruthy();
  cleanup();
  const own = setup("/plans/solo", null);
  act(() =>
    own.store.set(snapshotAtom, {
      session: null,
      turns: [],
      queue: [],
      previous: { session: ended, turns: [] },
    }),
  );
  expect(within(panel()).getByRole("log")).toBeTruthy();
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

it("another session's screen offers neither End chat nor New chat; its own screen offers New chat only", () => {
  setup("/f/beta/product", session());
  expect(within(panel()).queryByRole("button", { name: "End chat" })).toBeNull();
  expect(within(panel()).queryByRole("button", { name: "New chat" })).toBeNull();
  expect(within(panel()).queryByLabelText("Chat menu")).toBeNull();
  cleanup();
  setup("/f/alpha/product", session());
  expect(within(panel()).queryByRole("button", { name: "End chat", hidden: true })).toBeNull();
  expect(within(panel()).getByRole("button", { name: "New chat" })).toBeTruthy();
});

it("the c key does nothing while the panel has no composer to focus", () => {
  const away = setup("/f/beta/product", session());
  fireEvent.keyDown(document.body, { key: "c" });
  expect(away.store.get(focusComposerAtom)).toBe(false);
  cleanup();
  const home = setup("/f/alpha/product", session());
  fireEvent.keyDown(document.body, { key: "c" });
  expect(home.store.get(focusComposerAtom)).toBe(true);
});

it("the empty panel draws no workflow offer on any screen", async () => {
  const { store, router } = setup("/f/alpha", null);
  // The last visited feature: its checklist stays current on screens it does not own.
  act(() => store.set(activeFeatureAtom, "alpha"));
  const shown = () => within(panel()).queryAllByText(offer.label).length > 0;
  expect(shown()).toBe(false);
  for (const path of [
    "/plans/solo",
    "/files/product/beta/section_01.md",
    "/f/beta",
    "/f/alpha/product",
  ]) {
    await act(() => router.navigate(path));
    expect(shown()).toBe(false);
  }
});

it.each(["/", "/new"])("the c key sets no request on %s, which mounts no panel", (path) => {
  const { store } = setup(path, null);
  fireEvent.keyDown(document.body, { key: "c" });
  expect(store.get(focusComposerAtom)).toBe(false);
  expect(store.get(chatSheetOpenAtom)).toBe(false);
});

it("an offer with the open session on another screen raises no confirm and says why", async () => {
  const { store } = setup("/f/beta/product", session());
  await clickOffer();
  expect(window.confirm).not.toHaveBeenCalled();
  expect(endChat).not.toHaveBeenCalled();
  expect(startChat).not.toHaveBeenCalled();
  expect(within(panel()).getByRole("alert").textContent).toContain(
    "Finish the session in alpha first.",
  );
  expect(store.get(offerErrorAtom)).not.toBeNull();
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
  await act(() => router.navigate("/f/alpha/metaplan"));
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

// New chat starts nothing; the first Send starts the chat for the owner read here.
it.each([
  ["/f/beta/product", "beta", undefined],
  ["/plans/solo", undefined, "solo"],
  ["/files/plans/briefs/solo/common.md", undefined, "solo"],
  // Names no feature or plan exists for: the server would refuse them as a start's owner.
  ["/f/ghost", undefined, undefined],
  ["/plans/ghost", undefined, undefined],
  ["/files/plans/briefs/ghost/common.md", undefined, undefined],
])("a first send on %s starts the chat for feature %s, plan %s", (path, feature, plan) => {
  const { store } = setup(path, null);
  expect(store.get(startOwnerAtom)).toEqual({ feature, plan });
  expect(within(panel()).queryByRole("button", { name: "New chat" })).toBeNull();
  expect(startChat).not.toHaveBeenCalled();
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

// The wire carries no feature for this offer (`Offer::feature`), so the screen's names it.
it.each([["plan_follow_ups", "plan-follow-ups", "Plan follow-ups"]] as const)(
  "the %s offer chosen on a feature's screen starts for that feature",
  async (kind, id, label) => {
    const wire = offerViewSchema.parse({ id, kind, label, target: null });
    const { store } = setup("/f/alpha", null, wire);
    await clickOffer(wire);
    expect(startChat).toHaveBeenCalledWith(store, id, undefined, undefined, "alpha", undefined);
  },
);

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

it("the head reads New chat when empty, and the state line with a session", () => {
  setup("/f/beta/product", null);
  const head = () => within(panel()).getAllByRole("banner", { hidden: true })[0]!;
  const title = within(head()).getByRole("heading", { name: "New chat" });
  expect(title.hasAttribute("data-new")).toBe(true);
  expect(within(head()).queryByRole("button", { name: "New chat" })).toBeNull();
  cleanup();
  setup("/f/alpha/product", session({ state: "permission" }));
  const line = head().querySelector(".chat-state-line")!;
  expect(line.getAttribute("data-state")).toBe("permission");
  expect(within(head()).getByText("Needs your answer")).toBeTruthy();
  // The head offers New chat beside a running chat, and no menu: New chat ends it first.
  expect(within(head()).getByRole("button", { name: "New chat" })).toBeTruthy();
  expect(within(head()).queryByLabelText("Chat menu")).toBeNull();
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
  render(
    <Provider store={ended.store}>
      <RouterProvider router={createMemoryRouter([{ path: "*", element: <RealComposerArea /> }])} />
    </Provider>,
  );
  act(() => ended.store.set(addAttachmentAtom, draft));
  expect(screen.getByLabelText("Pending attachments")).toBeTruthy();
  cleanup();
  const open = setup("/f/beta/product", session());
  act(() => open.store.set(pendingAttachmentsAtom, [draft]));
  expect(within(panel()).queryByLabelText("Pending attachments")).toBeNull();
});

it("a concluded session on its own screen shows the pending chip beside New chat", () => {
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
