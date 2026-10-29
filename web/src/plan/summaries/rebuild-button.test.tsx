import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import helloFixture from "@/plan/api/fixtures/hello.json";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import { serverInfoSchema } from "@/plan/api/schema";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";

import { RebuildButton } from "./rebuild-button";

type Who = "holder" | "other" | "read_only";

const ADA = { public_id: "0123456789abcdef", name: "Ada" };
const DANA = { public_id: "bbbbbbbbbbbbbbbb", name: "Dana" };
const fetchMock = vi.fn<typeof fetch>();

function show(who: Who = "holder", ids = ["product"]) {
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
  return render(
    <Provider store={store}>
      <RebuildButton ids={ids} />
    </Provider>,
  );
}

function panelResponse() {
  return new Response(JSON.stringify(panelFixture), {
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
  show("holder", ["product", "product/section_01"]);
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
      body: JSON.stringify({ ids: ["product", "product/section_01"] }),
    }),
  );
  expect(screen.queryByRole("alert")).toBeNull();
});

it.each([
  ["other", "Dana holds authoring"],
  ["read_only", "loom plan is in read-only mode: tmux is not installed"],
] as const)("shows the reason and never posts for a %s browser", (who, reason) => {
  show(who);
  const button = screen.getByRole("button", { name: "Rebuild", description: reason });
  expect(button.getAttribute("aria-disabled")).toBe("true");
  expect(screen.getByText(reason).textContent).toBe(reason);
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
