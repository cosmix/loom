import { act, cleanup, render, screen } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import { connectionEpochAtom } from "@/plan/state/atoms";
import { ImplementingPlans, useImplementedBy } from "./turn-footer-plans";

const SECTION = "doc/product/demo/section_02.md";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function reply(title: string) {
  return new Response(
    JSON.stringify({
      sections: { "product/demo/section_02": [{ slug: "feat-ports", title, state: "written" }] },
    }),
  );
}

function Footer({ path }: { path: string }) {
  const loaded = useImplementedBy(true);
  return <ImplementingPlans path={path} loaded={loaded} />;
}

function mount(path = SECTION) {
  const store = createStore();
  render(
    <Provider store={store}>
      <MemoryRouter>
        <Footer path={path} />
      </MemoryRouter>
    </Provider>,
  );
  return store;
}

it("the first load shows one line of status, not a skeleton block", () => {
  vi.spyOn(globalThis, "fetch").mockReturnValue(new Promise(() => {}));
  mount();

  const status = screen.getByRole("status");

  expect(status.tagName).toBe("P");
  expect(status.textContent).toBe("Loading implementing plans…");
  expect(status.querySelector('[data-slot="skeleton"], .animate-pulse')).toBeNull();
});

it.each(["doc/product/demo/section_2.md", "doc/product/demo/section_002.md"])(
  "%s looks up the server's two-digit section id",
  async (path) => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(reply("Ports plan"));
    mount(path);

    expect(await screen.findByText("Ports plan")).toBeTruthy();
  },
);

it("a reconnect keeps the plans on show while they reload", async () => {
  const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(reply("Ports plan"));
  const store = mount();
  expect(await screen.findByText("Ports plan")).toBeTruthy();
  let finish!: (response: Response) => void;
  fetchSpy.mockImplementationOnce(
    () =>
      new Promise<Response>((resolve) => {
        finish = resolve;
      }),
  );

  act(() => store.set(connectionEpochAtom, 1));

  expect(fetchSpy).toHaveBeenCalledTimes(2);
  expect(screen.getByText("Ports plan")).toBeTruthy();
  expect(screen.queryByRole("status")).toBeNull();

  await act(async () => finish(reply("Ports plan, revised")));

  expect(await screen.findByText("Ports plan, revised")).toBeTruthy();
  expect(screen.queryByText("Ports plan")).toBeNull();
});
