import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { docChangedAtom } from "@/plan/files/atoms";

import {
  COMMIT,
  CURRENT_TEXT,
  FIRST_PLAN,
  HASH,
  JOB,
  S01,
  S02,
  S03,
  VISION_TEXT,
  deliver,
  kit,
  mount,
  patchDoc,
  resetKit,
  settlePosts,
} from "./components-kit";
import { SettleControl } from "./settle-control";

// The comparison's diff view loads CodeMirror; these tests read the two texts it is given.
vi.mock("@/plan/diff/diff-view", () => ({
  DiffView: ({ before, after }: { before: string | null; after: string | null }) => (
    <pre data-testid="diff">{`${before} => ${after}`}</pre>
  ),
}));

beforeEach(resetKit);

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

/** `fetch` that answers the first settled-version lookup with a 500, then as `fetch` does. */
function failingVersion(fetch: typeof globalThis.fetch): typeof globalThis.fetch {
  let failed = false;
  return (input, init) => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (!failed && raw.startsWith("/api/settled-version")) {
      failed = true;
      const body = JSON.stringify({ error: "internal", message: "git is busy" });
      const headers = { "content-type": "application/json" };
      return Promise.resolve(new Response(body, { status: 500, headers }));
    }
    return fetch(input, init);
  };
}

const buttons = () => screen.queryAllByRole("button").map((button) => button.textContent);
const seeChanged = () => screen.queryByRole("link", { name: "See what changed" });

async function control(path: string, name: string | RegExp) {
  const store = mount(<SettleControl feature="demo" path={path} hash={HASH} />);
  await screen.findByRole("button", { name });
  return store;
}

const STATES = [
  ["draft", "Mark settled", false],
  ["revised", "Changed since you settled it", true],
  ["stub_changed", "Changed since you settled it", true],
  ["upstream_changed", "Upstream changed since you settled it", false],
  ["settled", "Settled", false],
] as const;

for (const [state, label, link] of STATES) {
  it(`a ${state} section reads "${label}" and ${link ? "offers" : "has no"} See what changed`, async () => {
    patchDoc(S03, { settle: state });
    await control(S03, label);
    expect(buttons()).toEqual([label]);
    expect(seeChanged() !== null).toBe(link);
    expect(screen.queryByRole("link", { name: "Changes since settled" })).toBeNull();
  });
}

it("a plan whose spec moved says so and compares the revised section it covers", async () => {
  patchDoc(FIRST_PLAN, { settle: "upstream_changed" });
  patchDoc(S01, { settle: "revised" });
  await control(FIRST_PLAN, "Spec changed since you settled this plan");
  fireEvent.click(seeChanged()!);
  await screen.findByTestId("diff");
  const settled = kit.calls.find((call) => call.url.pathname === "/api/settled-version");
  expect(settled?.url.searchParams.get("path")).toBe("product/demo/section_01.md");
});

it("done work shows no settle state and no controls, even when it moved", async () => {
  const locked = "doc/plans/demo/DONE-PLAN-first.md";
  patchDoc(FIRST_PLAN, { path: locked, settle: "revised", settleable: false });
  mount(<SettleControl feature="demo" path={locked} hash={HASH} />);
  await waitFor(() =>
    expect(kit.calls.some((call) => call.url.pathname === "/api/trace/demo")).toBe(true),
  );
  expect(screen.queryByText(/^(Draft|Changed since you settled it|Settled)$/)).toBeNull();
  expect(buttons()).toEqual([]);
  expect(seeChanged()).toBeNull();
});

it("a settled document's button opens a menu holding Unsettle, which posts clear=1", async () => {
  patchDoc(S03, { settle: "settled" });
  await control(S03, "Settled");
  const trigger = screen.getByRole("button", { name: "Settled" });
  expect(trigger.getAttribute("aria-expanded")).toBe("false");
  fireEvent.click(trigger);
  const menu = await screen.findByRole("dialog", { name: "Settled" });
  expect(within(menu).queryByRole("button", { name: "Settle this version" })).toBeNull();
  fireEvent.click(within(menu).getByRole("button", { name: "Unsettle" }));
  await waitFor(() => expect(settlePosts()).toHaveLength(1));
  const [params] = settlePosts();
  expect(params!.get("clear")).toBe("1");
  expect(params!.get("path")).toBe("product/demo/section_03.md");
  expect(params!.get("hash")).toBe(HASH);
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});

it("a changed document's menu settles this version or unsettles it", async () => {
  patchDoc(S03, { settle: "revised" });
  await control(S03, "Changed since you settled it");
  fireEvent.click(screen.getByRole("button", { name: "Changed since you settled it" }));
  const menu = await screen.findByRole("dialog");
  expect(
    within(menu)
      .getAllByRole("button")
      .map((button) => button.textContent),
  ).toEqual(["Settle this version", "Unsettle"]);
  fireEvent.click(within(menu).getByRole("button", { name: "Settle this version" }));
  await waitFor(() => expect(settlePosts()).toHaveLength(1));
  expect(settlePosts()[0]!.get("clear")).toBeNull();
});

it("Escape closes the menu and returns focus to the button", async () => {
  patchDoc(S03, { settle: "settled" });
  await control(S03, "Settled");
  const trigger = screen.getByRole("button", { name: "Settled" });
  fireEvent.click(trigger);
  const menu = await screen.findByRole("dialog");
  fireEvent.keyDown(menu, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  await waitFor(() => expect(document.activeElement).toBe(trigger));
});

it("a narrow screen opens the menu as a bottom sheet", async () => {
  const narrow = (query: string) =>
    Object.assign(new EventTarget(), { media: query, matches: query.includes("max-width") });
  vi.stubGlobal("matchMedia", narrow);
  patchDoc(S03, { settle: "settled" });
  await control(S03, "Settled");
  fireEvent.click(screen.getByRole("button", { name: "Settled" }));
  const sheet = await screen.findByRole("dialog");
  expect(sheet.hasAttribute("data-sheet")).toBe(true);
  expect(within(sheet).getByRole("button", { name: "Unsettle" })).toBeTruthy();
});

it("the confirmation counts both kinds and quotes each with where it is", async () => {
  kit.docs["product/demo/section_03.md"] = {
    text: [
      "# Gamma",
      "",
      "## Coverage",
      "Every heading is covered or a gap. (My call.)",
      "",
      "## Open questions",
      "",
      "- Should a dismissed finding come back?",
    ].join("\n"),
    sections: [
      { level: 2, title: "Coverage", anchor: "coverage", start_line: 3, end_line: 5 },
      { level: 2, title: "Open questions", anchor: "open-questions", start_line: 6, end_line: 8 },
    ],
  };
  kit.settleAnswers = [
    {
      status: 409,
      body: { error: "unresolved", message: "unresolved points", my_calls: 1, open_questions: 1 },
    },
  ];
  await control(S03, "Mark settled");
  fireEvent.click(screen.getByRole("button", { name: "Mark settled" }));
  const dialog = await screen.findByRole("dialog", { name: "Settle with open items?" });
  expect(dialog.textContent).toContain("1 My call and 1 open question");
  await waitFor(() => expect(dialog.textContent).toContain("Every heading is covered or a gap."));
  expect(dialog.textContent).not.toContain("(My call");
  expect(within(dialog).getByRole("link", { name: "Coverage" }).getAttribute("href")).toBe(
    "/f/demo/source/product/demo/section_03.md#coverage",
  );
  expect(dialog.textContent).toContain("Should a dismissed finding come back?");
  expect(dialog.textContent).toContain("Open question 1");
});

it("the confirmation shows the counts alone while the text is not loaded", async () => {
  kit.settleAnswers = [
    {
      status: 409,
      body: { error: "unresolved", message: "unresolved points", my_calls: 2, open_questions: 1 },
    },
  ];
  await control(S03, "Mark settled");
  fireEvent.click(screen.getByRole("button", { name: "Mark settled" }));
  const dialog = await screen.findByRole("dialog");
  expect(dialog.textContent).toContain("2 My calls and 1 open question");
  expect(within(dialog).queryByRole("list")).toBeNull();
  fireEvent.click(within(dialog).getByRole("button", { name: "Not yet" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(settlePosts()).toHaveLength(1);
});

it("a conflict reloads the document and the trace and asks to settle again", async () => {
  kit.settleAnswers = [
    { status: 409, body: { result: "conflict", current_text: "x", current_hash: "sha256:newer" } },
  ];
  const store = await control(S03, "Mark settled");
  fireEvent.click(screen.getByRole("button", { name: "Mark settled" }));
  await screen.findByText(/changed while you were reading it/);
  expect(store.get(docChangedAtom)).toEqual({ [S03]: "sha256:newer" });
  // The trace was fetched on mount and again after the answer.
  await waitFor(() =>
    expect(kit.calls.filter((call) => call.url.pathname === "/api/trace/demo")).toHaveLength(2),
  );
});

it("a settled line that was never committed disables See what changed and says why", async () => {
  kit.settledCommit = null;
  patchDoc(S02, { settle: "revised" });
  await control(S02, "Changed since you settled it");
  const reason = await screen.findByText(/was never committed, so there is nothing to compare/);
  const link = screen.getByRole("link", { name: "See what changed" });
  expect(link.getAttribute("aria-disabled")).toBe("true");
  expect(link.getAttribute("aria-describedby")).toBe(reason.id);
  expect(reason.className).toContain("plan-fine");
  expect(reason.textContent).toContain("Settle again to record it.");
  fireEvent.click(link);
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(kit.calls.some((call) => call.url.pathname === "/api/at-commit")).toBe(false);
});

it("See what changed asks for the settled version on mount and waits disabled", async () => {
  patchDoc(S02, { settle: "revised" });
  await control(S02, "Changed since you settled it");
  const link = screen.getByRole("link", { name: "See what changed" });
  const asked = () => kit.calls.filter((call) => call.url.pathname === "/api/settled-version");
  expect(asked()).toHaveLength(1);
  await waitFor(() => expect(link.getAttribute("aria-disabled")).toBeNull());
  expect(link.getAttribute("aria-describedby")).toBeNull();
});

it("a failed lookup says so quietly and asks again on a press", async () => {
  patchDoc(S02, { settle: "revised" });
  vi.stubGlobal("fetch", failingVersion(globalThis.fetch));
  await control(S02, "Changed since you settled it");
  const reason = await screen.findByText((_, node) =>
    Boolean(node?.matches(".trace-changes-reason") && node.textContent?.includes("git is busy")),
  );
  expect(reason.textContent).toContain("Select See what changed to try again.");
  const link = screen.getByRole("link", { name: "See what changed" });
  expect(link.getAttribute("aria-disabled")).toBeNull();
  expect(link.getAttribute("aria-describedby")).toBe(reason.id);
  fireEvent.click(link);
  await waitFor(() => expect(link.getAttribute("aria-describedby")).toBeNull());
  expect(screen.queryByRole("dialog")).toBeNull();
  await waitFor(() => expect(link.getAttribute("aria-disabled")).toBeNull());
  fireEvent.click(link);
  await screen.findByTestId("diff");
});

it("See what changed compares the settled commit with the file now", async () => {
  patchDoc(S02, { settle: "revised" });
  await control(S02, "Changed since you settled it");
  fireEvent.click(screen.getByRole("link", { name: "See what changed" }));
  const diff = await screen.findByTestId("diff");
  expect(diff.textContent).toBe(`settled text => ${CURRENT_TEXT}`);
  const atCommit = kit.calls.find((call) => call.url.pathname === "/api/at-commit");
  expect(atCommit?.url.searchParams.get("commit")).toBe(kit.settledCommit);
});

it("a changed stub compares the vision file, against the section's settled commit", async () => {
  patchDoc(S02, { settle: "stub_changed" });
  await control(S02, "Changed since you settled it");
  fireEvent.click(screen.getByRole("link", { name: "See what changed" }));
  const diff = await screen.findByTestId("diff");
  expect(diff.textContent).toBe(`settled text => ${VISION_TEXT}`);
  const settled = kit.calls.find((call) => call.url.pathname === "/api/settled-version");
  expect(settled?.url.searchParams.get("path")).toBe("product/demo/section_02.md");
});

/** A `settle_commit` frame: `path`'s commit ended, committed. */
const committedFrame = (path: string) => ({
  type: "settle_commit",
  job: JOB,
  path,
  state: "committed",
  commit: COMMIT,
  reason: null,
});

const versionAsks = () => kit.calls.filter((call) => call.url.pathname === "/api/settled-version");

it("See what changed asks again once a settle commit of its document ends", async () => {
  patchDoc(S02, { settle: "revised" });
  await control(S02, "Changed since you settled it");
  await waitFor(() => expect(versionAsks()).toHaveLength(1));
  deliver(committedFrame(S03));
  expect(versionAsks()).toHaveLength(1);
  deliver(committedFrame(S02));
  await waitFor(() => expect(versionAsks()).toHaveLength(2));
});

it("an overtaken settled-version answer never replaces the later one", async () => {
  patchDoc(S02, { settle: "revised" });
  kit.settledCommit = null;
  let release = () => {};
  kit.versionGate = new Promise((resolve) => {
    release = resolve;
  });
  await control(S02, "Changed since you settled it");
  await waitFor(() => expect(versionAsks()).toHaveLength(1));
  kit.settledCommit = COMMIT;
  deliver(committedFrame(S02));
  const link = screen.getByRole("link", { name: "See what changed" });
  await waitFor(() => expect(link.getAttribute("aria-disabled")).toBeNull());
  release();
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(link.getAttribute("aria-disabled")).toBeNull();
  expect(screen.queryByText(/never committed/)).toBeNull();
});
