import { cleanup, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it } from "vitest";

import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import { serverInfoSchema } from "@/plan/api/schema";
import { Banners } from "@/plan/header/banners";
import { serverInfoAtom, stoppingAtom } from "@/plan/state/atoms";

afterEach(cleanup);

function renderBanners(setup: (store: ReturnType<typeof createStore>) => void) {
  const store = createStore();
  setup(store);
  render(
    <Provider store={store}>
      <Banners />
    </Provider>,
  );
}

describe("Banners", () => {
  it("lists each missing prerequisite with its fix in one read-only banner", () => {
    renderBanners((store) => store.set(serverInfoAtom, serverInfoSchema.parse(serverInfoFixture)));
    expect(screen.getAllByRole("status")).toHaveLength(1);
    expect(screen.getByText(/Claude is not logged in\. Log in to Claude/)).toBeTruthy();
  });

  it("warns for the life of the page on a remote bind", () => {
    renderBanners((store) =>
      store.set(serverInfoAtom, {
        ...serverInfoSchema.parse(serverInfoFixture),
        remote: true,
        read_only: [],
      }),
    );
    expect(
      screen.getByText(
        /This server is plain HTTP\. Anyone who can see the traffic can read the link/,
      ),
    ).toBeTruthy();
  });

  it("announces stopping", () => {
    renderBanners((store) => store.set(stoppingAtom, true));
    expect(screen.getByText("loom plan is stopping.")).toBeTruthy();
  });
});
