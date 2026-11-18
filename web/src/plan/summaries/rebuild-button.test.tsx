import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import helloFixture from "@/plan/api/fixtures/hello.json";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import { serverInfoSchema } from "@/plan/api/schema";
import { summaryPanelSchema } from "@/plan/api/schema-summaries";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";

import { RebuildButton } from "./rebuild-button";
import { panelLoadAtom } from "./state";

type Who = "holder" | "other" | "read_only";

const ADA = { public_id: "0123456789abcdef", name: "Ada" };
const DANA = { public_id: "bbbbbbbbbbbbbbbb", name: "Dana" };
const fetchMock = vi.fn<typeof fetch>();

function show(who: Who = "holder", ids = ["overview/demo/product"], rebuilding = false) {
  const store = createStore();
  initClient(store);
  store.set(youAtom, ADA);
  store.set(
    serverInfoAtom,
    serverInfoSchema.parse({
      ...helloFixture.server,
      read_only:
        who === "read_only"
          ? [{ what: "tmux", message: "tmux is not installed", fix: "Install tmux" }]
          : [],
      you: ADA,
    }),
  );
  const holder = who === "read_only" ? null : who === "holder" ? ADA : DANA;
  store.set(authoringAtom, {
    holder: holder && { ...holder, away_since: null, frees_at: null },
  });
  render(
    <Provider store={store}>
      <RebuildButton ids={ids} rebuilding={rebuilding} />
    </Provider>,
  );
  return { store };
}

/** The fixture runs `overview/demo/product`; `idle` is the panel once that rebuild has finished. */
function panelResponse(idle = true) {
  const panel = idle ? { ...panelFixture, running: null, queue: [] } : panelFixture;
  return new Response(JSON.stringify(panel), {
    status: 202,
    headers: { "content-type": "application/json" },
  });
}

beforeEach(() => {
  fetchMock.mockReset();
  fetchMock.mockImplementation(async () => panelResponse());
  vi.stubGlobal("fetch", fetchMock);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("posts the requested ids for the holder", async () => {
  show("holder", ["overview/demo/product", "demo/product/section_01"]);
  const button = screen.getByRole("button", { name: "Rebuild" });
  expect(button.getAttribute("aria-disabled")).not.toBe("true");
  await act(async () => {
    fireEvent.click(button);
  });

  await waitFor(() => expect(fetchMock).toHaveBeenCalledOnce());
  expect(fetchMock).toHaveBeenCalledWith(
    "/api/summaries/rebuild",
    expect.objectContaining({
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ids: ["overview/demo/product", "demo/product/section_01"] }),
    }),
  );
  expect(screen.queryByRole("alert")).toBeNull();
});

it.each([
  ["other", "Dana holds authoring"],
  ["read_only", "Read-only"],
] as const)("shows the reason and never posts for a %s browser", (who, reason) => {
  show(who);
  const button = screen.getByRole("button", { name: "Rebuild", description: reason });
  expect(button.getAttribute("aria-disabled")).toBe("true");
  expect(screen.getByText(reason).textContent).toBe(reason);
  // The reason follows the button on its line, beside the summary status, never stacked under it.
  expect(button.nextElementSibling?.textContent).toBe(reason);
  expect(button.parentElement?.classList.contains("flex-col")).toBe(false);
  button.focus();
  expect(document.activeElement).toBe(button);
  fireEvent.click(button);
  expect(fetchMock).not.toHaveBeenCalled();
});

it("ignores another press while pending and accepts one after completion", async () => {
  let finish: (response: Response) => void = () => {
    throw new Error("Request has not started");
  };
  fetchMock.mockImplementationOnce(
    () =>
      new Promise<Response>((resolve) => {
        finish = resolve;
      }),
  );
  show();
  const button = screen.getByRole("button", { name: "Rebuild" });
  act(() => {
    fireEvent.click(button);
    fireEvent.click(button);
  });
  expect(fetchMock).toHaveBeenCalledOnce();
  await act(async () => finish(panelResponse()));
  await act(async () => {
    fireEvent.click(button);
  });
  expect(fetchMock).toHaveBeenCalledTimes(2);
});

it("reads Rebuilding and stays inert while its summary runs, then turns back into Rebuild", async () => {
  let finish: (response: Response) => void = () => {
    throw new Error("Request has not started");
  };
  fetchMock.mockImplementationOnce(
    () =>
      new Promise<Response>((resolve) => {
        finish = resolve;
      }),
  );
  const { store } = show();
  const button = screen.getByRole("button", { name: "Rebuild" });
  act(() => fireEvent.click(button));
  // In flight: the label changes at once, before the server answers.
  expect(button.textContent).toBe("Rebuilding…");
  expect(button.getAttribute("aria-disabled")).toBe("true");
  expect(button.querySelector(".plan-busy-text svg")).not.toBeNull();
  // The server queued it: the panel keeps it busy after the request returns.
  await act(async () => finish(panelResponse(false)));
  expect(button.textContent).toBe("Rebuilding…");
  fireEvent.click(button);
  expect(fetchMock).toHaveBeenCalledOnce();
  // The push that drops the id ends it.
  act(() =>
    store.set(panelLoadAtom, {
      status: "ready",
      panel: summaryPanelSchema.parse({ ...panelFixture, running: null, queue: [] }),
    }),
  );
  expect(button.textContent).toBe("Rebuild");
  expect(button.getAttribute("aria-disabled")).toBeNull();
});

it("stays a Rebuild press while its summary waits for edits to stop", () => {
  const { store } = show("holder", ["demo/product/section_03"]);
  act(() =>
    store.set(panelLoadAtom, {
      status: "ready",
      panel: summaryPanelSchema.parse({
        ...panelFixture,
        queue: ["demo/product/section_03"],
        waiting: ["demo/product/section_03"],
      }),
    }),
  );
  const button = screen.getByRole("button", { name: "Rebuild" });
  expect(button.getAttribute("aria-disabled")).toBeNull();
});

it("reads Rebuilding while the page reports its summary rebuilding", () => {
  show("holder", ["overview/demo/product"], true);
  const button = screen.getByRole("button", { name: "Rebuilding…" });
  expect(button.getAttribute("aria-disabled")).toBe("true");
  expect(screen.queryByText("Read-only")).toBeNull();
});

it("shows a server error and clears it as soon as the next press starts", async () => {
  fetchMock.mockResolvedValueOnce(
    new Response(JSON.stringify({ error: "start_failed", message: "Could not start rebuild" }), {
      status: 500,
      headers: { "content-type": "application/json" },
    }),
  );
  let finish: (response: Response) => void = () => {
    throw new Error("Retry has not started");
  };
  fetchMock.mockImplementationOnce(
    () =>
      new Promise<Response>((resolve) => {
        finish = resolve;
      }),
  );
  show();
  const button = screen.getByRole("button", { name: "Rebuild" });
  fireEvent.click(button);
  expect((await screen.findByRole("alert")).textContent).toBe("Could not start rebuild");

  fireEvent.click(button);
  expect(fetchMock).toHaveBeenCalledTimes(2);
  expect(screen.queryByRole("alert")).toBeNull();
  await act(async () => finish(panelResponse()));
  expect(screen.queryByRole("alert")).toBeNull();
});
