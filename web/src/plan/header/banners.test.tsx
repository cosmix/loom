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

  it("joins each message and fix with one full stop, whatever the message ends with", () => {
    renderBanners((store) =>
      store.set(serverInfoAtom, {
        ...serverInfoSchema.parse(serverInfoFixture),
        read_only: [
          { what: "sandbox", message: "bubblewrap is not installed.", fix: "Install bubblewrap" },
          {
            what: "claude_login",
            message: "Claude is not logged in",
            fix: "Run claude auth login",
          },
        ],
      }),
    );
    const banner = screen.getByRole("status");
    expect(banner.textContent).toContain("bubblewrap is not installed. Install bubblewrap");
    expect(banner.textContent).toContain("Claude is not logged in. Run claude auth login");
    expect(banner.textContent).not.toContain("..");
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

  it("announces stopping once when the server snapshot says so", () => {
    renderBanners((store) =>
      store.set(serverInfoAtom, {
        ...serverInfoSchema.parse(serverInfoFixture),
        read_only: [],
        stopping: true,
      }),
    );
    expect(screen.getAllByText("loom plan is stopping.")).toHaveLength(1);
  });
});
