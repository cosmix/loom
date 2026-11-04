import { render, screen, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { createMemoryRouter, RouterProvider } from "react-router";
import { beforeEach, expect, it, vi } from "vitest";

import { routes } from "@/plan/router";
import { checkpointAtom } from "@/plan/review/checkpoint";
import {
  A,
  checkpoint,
  current,
  installChangedSinceLifecycle,
  replies,
  server,
} from "@/plan/review/changed-since-test-kit";
import { hashesAtom } from "@/plan/review/live";
import { planSocketAtom, serverInfoAtom } from "@/plan/state/atoms";

installChangedSinceLifecycle();

beforeEach(() => {
  vi.spyOn(window, "matchMedia").mockImplementation(
    (query) =>
      ({
        matches: true,
        media: query,
        addEventListener() {},
        removeEventListener() {},
      }) as unknown as MediaQueryList,
  );
  // One changed document: A, whose hash moved from the checkpoint's.
  const only = { head: current.head, docs: { [A]: current.docs[A] } };
  Object.assign(replies, { "/api/hashes": only });
});

function store() {
  const value = createStore();
  value.set(serverInfoAtom, { ...server, project_key: "changes-page" });
  value.set(checkpointAtom, { commit: checkpoint.commit, hashes: { [A]: checkpoint.hashes[A]! } });
  value.set(hashesAtom, { head: current.head, docs: { [A]: current.docs[A]! } });
  value.set(planSocketAtom, {
    send: vi.fn(() => true),
    subscribe: vi.fn(() => () => {}),
    dispose: vi.fn(),
    onResync: () => () => {},
  });
  return value;
}

it("Changed since you looked is a page", async () => {
  const router = createMemoryRouter(routes, { initialEntries: ["/changes"] });
  render(
    <Provider store={store()}>
      <RouterProvider router={router} />
    </Provider>,
  );

  expect(
    await screen.findByRole("heading", { level: 1, name: "Changed since you looked" }),
  ).toBeTruthy();
  expect(screen.queryByRole("dialog")).toBeNull();
  const list = await screen.findByRole("list", { name: "Changed documents" });
  expect(await within(list).findByRole("button", { name: `Show diff for ${A}` })).toBeTruthy();
});
