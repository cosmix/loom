import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { afterEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import hello from "@/plan/api/fixtures/hello.json";
import stripFixture from "@/plan/api/fixtures/run-strip.json";
import { runStripSchema } from "@/plan/api/schema-run";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";

import { DashboardButton } from "./dashboard-button";
import { runStripAtom } from "./use-run";

function renderDashboard(holder = true, variant?: "full" | "link") {
  const store = createStore();
  store.set(serverInfoAtom, { ...hello.server, read_only: [] });
  store.set(youAtom, hello.you);
  store.set(authoringAtom, {
    holder: {
      ...hello.you,
      public_id: holder ? hello.you.public_id : "other",
      away_since: null,
      frees_at: null,
    },
  });
  initClient(store);
  return {
    store,
    ...render(
      <Provider store={store}>
        <DashboardButton variant={variant} />
      </Provider>,
    ),
  };
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("renders nothing until a dashboard URL is known", () => {
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>(() => new Promise(() => {})),
  );
  const { container } = renderDashboard();
  expect(container.innerHTML).toBe("");
});

it("posts on mount and renders a safe anchor and the tunnel hint as text", async () => {
  const fetchSpy = vi.fn<typeof fetch>(
    async () =>
      new Response(
        JSON.stringify({
          url: "http://localhost:7373/?token=holder-token",
          tunnel: "ssh -L 7373:localhost:7373 host <example>",
        }),
      ),
  );
  vi.stubGlobal("fetch", fetchSpy);
  renderDashboard();
  const link = await screen.findByRole("link", { name: "Open dashboard" });
  expect(link.tagName).toBe("A");
  expect(link.getAttribute("target")).toBe("_blank");
  expect(link.getAttribute("rel")).toBe("noopener noreferrer");
  expect(link.getAttribute("href")).toBe("http://localhost:7373/?token=holder-token");
  expect(screen.getByText("ssh -L 7373:localhost:7373 host <example>")).toBeTruthy();
  expect(fetchSpy).toHaveBeenCalledWith(
    "/api/run/dashboard",
    expect.objectContaining({ method: "POST" }),
  );
});

it("fetches again on each run strip change", async () => {
  const fetchSpy = vi.fn<typeof fetch>(
    async () => new Response(JSON.stringify({ url: "http://localhost:7373/" })),
  );
  vi.stubGlobal("fetch", fetchSpy);
  const { store } = renderDashboard();
  await screen.findByRole("link", { name: "Open dashboard" });
  act(() => store.set(runStripAtom, runStripSchema.parse(stripFixture)));
  await waitFor(() => expect(fetchSpy).toHaveBeenCalledTimes(2));
  act(() => store.set(runStripAtom, { ...runStripSchema.parse(stripFixture), merged: 2 }));
  await waitFor(() => expect(fetchSpy).toHaveBeenCalledTimes(3));
});

it("a viewer's dashboard anchor has no token", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>(
      async () => new Response(JSON.stringify({ url: "http://localhost:7373/" })),
    ),
  );
  renderDashboard(false);
  const link = await screen.findByRole("link", { name: "Open dashboard" });
  expect(link.getAttribute("href")).not.toContain("token=");
});

it("discards a holder URL if authoring changes and the refresh fails", async () => {
  const fetchSpy = vi
    .fn<typeof fetch>()
    .mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          url: "http://localhost:7373/?token=holder-token",
        }),
      ),
    )
    .mockResolvedValue(
      new Response(JSON.stringify({ error: "unreachable", message: "Unavailable" }), {
        status: 503,
      }),
    );
  vi.stubGlobal("fetch", fetchSpy);
  const { store } = renderDashboard();
  await screen.findByRole("link", { name: "Open dashboard" });
  act(() =>
    store.set(authoringAtom, {
      holder: { ...hello.you, public_id: "other", away_since: null, frees_at: null },
    }),
  );
  await waitFor(() => expect(fetchSpy).toHaveBeenCalledTimes(2));
  expect(screen.queryByRole("link", { name: "Open dashboard" })).toBeNull();
});

it("shows the failure when the dashboard is unavailable and no url is known", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>(
      async () =>
        new Response(
          JSON.stringify({ error: "dashboard_unavailable", message: "Dashboard did not start" }),
          { status: 502 },
        ),
    ),
  );
  renderDashboard();
  expect(await screen.findByText("Dashboard did not start")).toBeTruthy();
  expect(screen.queryByRole("link", { name: "Open dashboard" })).toBeNull();
});

// Let the mocked fetch and the state update it triggers finish before asserting an absence.
const settle = () =>
  act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 20));
  });

const withTunnel = () =>
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>(
      async () =>
        new Response(
          JSON.stringify({
            url: "http://localhost:7373/",
            tunnel: "ssh -L 7373:localhost:7373 host",
          }),
        ),
    ),
  );

it("styles the link as a link", async () => {
  withTunnel();
  renderDashboard();
  const link = await screen.findByRole("link", { name: "Open dashboard" });
  expect(link.classList.contains("underline")).toBe(true);
});

it("keeps the tunnel line out of the wide header form", async () => {
  withTunnel();
  renderDashboard(true, "link");
  await screen.findByRole("link", { name: "Open dashboard" });
  expect(screen.queryByText(/ssh -L/)).toBeNull();
});

it("shows no failure panel in the header forms", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>(
      async () =>
        new Response(
          JSON.stringify({ error: "dashboard_unavailable", message: "Dashboard did not start" }),
          {
            status: 502,
          },
        ),
    ),
  );
  const { container } = renderDashboard(true, "link");
  await settle();
  expect(container.innerHTML).toBe("");
});

it.each(["javascript:alert(1)", "data:text/html,x", "file:///etc/passwd", "//evil.example/"])(
  "does not render a link for the non-web URL %s",
  async (url) => {
    vi.stubGlobal(
      "fetch",
      vi.fn<typeof fetch>(async () => new Response(JSON.stringify({ url }))),
    );
    const { container } = renderDashboard();
    await settle();
    expect(screen.queryByRole("link")).toBeNull();
    expect(container.innerHTML).toBe("");
  },
);
