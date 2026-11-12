import { act, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { stoppingAtom } from "@/plan/state/atoms";

import {
  FIRST_PLAN,
  S01,
  S02,
  S03,
  json,
  kit,
  mount,
  patchDoc,
  resetKit,
  type Answer,
} from "./components-kit";
import { SettleAllButton } from "./settle-all";

let answers: Answer[];
let posts: { query: URLSearchParams; body: unknown }[];
let gate: Promise<void> | null;

beforeEach(() => {
  resetKit();
  answers = [];
  posts = [];
  gate = null;
  const fetch = globalThis.fetch;
  vi.stubGlobal("fetch", async (input: RequestInfo | URL, init?: RequestInit) => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const url = new URL(raw, "http://localhost");
    if (url.pathname !== "/api/settle-all") return fetch(input, init);
    posts.push({ query: url.searchParams, body: init?.body });
    if (gate) await gate;
    const next = answers.shift() ?? { status: 200, body: { settled: [], skipped: [] } };
    return json(next.body, next.status);
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("hides when no document of the role needs settling", async () => {
  patchDoc(S01, { settle: "settled" });
  patchDoc(S02, { settleable: false });
  patchDoc(S03, { settleable: false });
  mount(<SettleAllButton feature="demo" role="section" />);
  await waitFor(() =>
    expect(kit.calls.some((call) => call.url.pathname === "/api/trace/demo")).toBe(true),
  );
  expect(screen.queryByRole("button")).toBeNull();
});

it("counts only settleable unsettled documents of the requested role", () => {
  patchDoc(S01, { settle: "settled" });
  patchDoc(S02, { settle: "revised" });
  patchDoc(S03, { settleable: false });
  mount(<SettleAllButton feature="demo" role="section" />);
  expect(screen.getByRole("button", { name: "Settle all specs" })).toBeTruthy();
  expect(screen.getByText("1 spec needs settling")).toBeTruthy();
});

it("offers confirmation for the unresolved count and posts confirm only on that press", async () => {
  answers = [
    {
      status: 200,
      body: { settled: [S02], skipped: [{ path: S03, reason: "unresolved" }] },
    },
  ];
  mount(<SettleAllButton feature="demo" role="section" />);
  fireEvent.click(screen.getByRole("button", { name: "Settle all specs" }));
  expect(await screen.findByText("1 document with unresolved points.")).toBeTruthy();
  expect(posts).toHaveLength(1);
  expect(Object.fromEntries(posts[0]!.query)).toEqual({ feature: "demo", role: "section" });
  expect(posts[0]!.body).toBe("");
  fireEvent.click(screen.getByRole("button", { name: "Settle anyway" }));
  await waitFor(() => expect(posts).toHaveLength(2));
  expect(posts[1]!.query.get("confirm")).toBe("1");
  await waitFor(() => expect(screen.queryByRole("button", { name: "Settle anyway" })).toBeNull());
});

it("shows the authoring block reason and cannot post while blocked", () => {
  const store = mount(<SettleAllButton feature="demo" role="section" />);
  act(() => store.set(stoppingAtom, true));
  const button = screen.getByRole("button", { name: "Settle all specs" });
  expect(button.getAttribute("aria-disabled")).toBe("true");
  expect(document.getElementById(button.getAttribute("aria-describedby")!)?.textContent).toBe(
    "loom plan is stopping",
  );
  fireEvent.click(button);
  expect(posts).toHaveLength(0);
});

it("posts plans and prevents duplicate presses while working", async () => {
  patchDoc(FIRST_PLAN, { settle: "draft", settleable: true });
  let release!: () => void;
  gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  mount(<SettleAllButton feature="demo" role="plan" />);
  const button = screen.getByRole("button", { name: "Settle all plans" });
  fireEvent.click(button);
  fireEvent.click(button);
  expect(posts).toHaveLength(1);
  expect(posts[0]!.query.get("role")).toBe("plan");
  expect(button.getAttribute("aria-busy")).toBe("true");
  await act(async () => release());
  await waitFor(() => expect(button.getAttribute("aria-busy")).toBeNull());
});

it("shows failed requests and skipped stale documents", async () => {
  answers = [
    { status: 500, body: { error: "internal", message: "Could not settle documents" } },
    { status: 200, body: { settled: [], skipped: [{ path: S02, reason: "stale" }] } },
  ];
  mount(<SettleAllButton feature="demo" role="section" />);
  fireEvent.click(screen.getByRole("button", { name: "Settle all specs" }));
  expect((await screen.findByRole("alert")).textContent).toBe("Could not settle documents");
  fireEvent.click(screen.getByRole("button", { name: "Settle all specs" }));
  expect(
    await screen.findByText("1 document could not be settled. Reload and try again."),
  ).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Settle anyway" })).toBeNull();
});
