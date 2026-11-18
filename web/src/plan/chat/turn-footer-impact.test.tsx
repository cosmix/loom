import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import snapshot from "@/plan/api/fixtures/chat-snapshot.json";
import traceFixture from "@/plan/api/fixtures/trace.json";
import { chatSnapshotSchema, type Turn } from "@/plan/api/schema-chat";
import { traceViewSchema } from "@/plan/api/schema-trace";

import { TurnFooter } from "./turn-footer";

// The footer's restore controls need a held session; these tests read the rows beside them.
vi.mock("./restore", () => ({ RestoreButton: () => null, RestoreAll: () => null }));

const S01 = "doc/product/demo/section_01.md";

let implementedBy: Record<string, { slug: string; title: string; state: string }[]>;

function json(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

beforeEach(() => {
  implementedBy = {};
  vi.stubGlobal("fetch", (input: RequestInfo | URL): Promise<Response> => {
    const { pathname } = new URL(String(input), "http://localhost");
    if (pathname === "/api/trace/demo")
      return Promise.resolve(json(traceViewSchema.parse(traceFixture)));
    if (pathname === "/api/implemented-by")
      return Promise.resolve(json({ sections: implementedBy }));
    return Promise.resolve(json({ path: S01, before: "old\n", after: "new\n" }));
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function footer(): Turn {
  return {
    ...chatSnapshotSchema.parse(snapshot).turns[0]!,
    id: "t2",
    open: false,
    changed_files: [{ path: S01, before_hash: "before", by_tool: true }],
    summaries_rebuilding: [],
  };
}

function mount() {
  const store = createStore();
  initClient(store);
  render(
    <Provider store={store}>
      <MemoryRouter>
        <TurnFooter turn={footer()} sessionId="session one" />
      </MemoryRouter>
    </Provider>,
  );
}

it("a plan that ImplementingPlans draws is not listed again among the affected documents", async () => {
  implementedBy = {
    "product/demo/section_01": [{ slug: "first", title: "First plan", state: "written" }],
  };
  mount();
  const group = await screen.findByRole("group", { name: "written plans" });
  expect(within(group).getByText("First plan")).toBeTruthy();
  await waitFor(() => expect(screen.queryByText("Loading implementing plans…")).toBeNull());
  const affected = screen.queryByRole("list", { name: "Affected documents" });
  expect(affected?.textContent ?? "").not.toMatch(/\bfirst\b/i);
});

it("a covering plan that ImplementingPlans lacks stays among the affected documents", async () => {
  mount();
  const affected = await screen.findByRole("list", { name: "Affected documents" });
  expect(affected.textContent).toMatch(/\bfirst\b/i);
});
