import { act, cleanup, fireEvent, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { afterEach, describe, expect, it, vi } from "vitest";

import hello from "@/plan/api/fixtures/hello.json";
import type { ClientMessage, PlanMessage } from "@/plan/api/schema";
import type { connectPlanSocket } from "@/plan/api/socket";
import { snapshotAtom } from "@/plan/chat/model";
import { authoringAtom, draftTeamAtom, planSocketAtom, serverInfoAtom } from "@/plan/state/atoms";
import { paths, renderPanel, server, session, storeFor } from "./draft-team-panel-kit";

function installSocket(store: ReturnType<typeof createStore>) {
  const handlers = new Set<(message: PlanMessage) => void>();
  const send = vi.fn((_message: ClientMessage) => true);
  const socket: ReturnType<typeof connectPlanSocket> = {
    send,
    subscribe(type, handler) {
      const dispatch = (message: PlanMessage) => {
        if (message.type === type) handler(message as Parameters<typeof handler>[0]);
      };
      handlers.add(dispatch);
      return () => {
        handlers.delete(dispatch);
      };
    },
    onResync: () => () => {},
    dispose: () => {},
  };
  store.set(planSocketAtom, socket);
  const reply = (error: string | null = null) => {
    const request = send.mock.calls.at(-1)?.[0];
    if (request?.type !== "chat") throw new Error("Expected a chat command");
    const message: PlanMessage = {
      type: "chat",
      frame: "result",
      client_id: request.client_id,
      ok: error === null,
      error,
    };
    [...handlers].forEach((handler) => handler(message));
  };
  return { send, reply };
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("DraftTeamPanel", () => {
  it("confirms before sending the end command and prevents duplicate stops", async () => {
    const store = storeFor();
    const socket = installSocket(store);
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    renderPanel(store);

    const button = screen.getByRole("button", { name: "Stop the team" });
    fireEvent.click(button);
    fireEvent.click(button);

    expect(confirm).toHaveBeenCalledExactlyOnceWith(
      "Stop the team? Documents already written stay.",
    );
    expect(socket.send).toHaveBeenCalledExactlyOnceWith({
      type: "chat",
      command: "end",
      client_id: expect.any(String),
    });
    expect(button.getAttribute("aria-disabled")).toBe("true");
    expect(screen.getByText("Stopping the team")).toBeTruthy();
    await act(async () => socket.reply());
    expect(button.getAttribute("aria-disabled")).toBeNull();
    expect(store.get(draftTeamAtom)?.documents).toEqual(paths);
  });

  it("does not send when confirmation is declined", () => {
    const store = storeFor();
    const socket = installSocket(store);
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    renderPanel(store);

    fireEvent.click(screen.getByRole("button", { name: "Stop the team" }));

    expect(confirm).toHaveBeenCalledExactlyOnceWith(
      "Stop the team? Documents already written stay.",
    );
    expect(socket.send).not.toHaveBeenCalled();
  });

  it("does not end another session that becomes current during confirmation", () => {
    const store = storeFor();
    const socket = installSocket(store);
    vi.spyOn(window, "confirm").mockImplementation(() => {
      store.set(snapshotAtom, {
        ...store.get(snapshotAtom),
        session: { ...session, id: "vision" },
      });
      return true;
    });
    renderPanel(store);

    fireEvent.click(screen.getByRole("button", { name: "Stop the team" }));

    expect(socket.send).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "Stop the team" })).toBeNull();
  });

  it("shows the read-only reason and a stop button that cannot send", () => {
    const store = storeFor();
    store.set(serverInfoAtom, server);
    const socket = installSocket(store);
    const confirm = vi.spyOn(window, "confirm");
    renderPanel(store);

    const button = screen.getByRole("button", { name: "Stop the team" });
    expect(screen.getAllByRole("button")).toHaveLength(1);
    fireEvent.click(button);

    expect(button.getAttribute("aria-disabled")).toBe("true");
    expect(screen.getByText("Stopping the team requires authoring")).toBeTruthy();
    expect(
      screen.getByText("loom plan is in read-only mode: Claude is not logged in"),
    ).toBeTruthy();
    expect(confirm).not.toHaveBeenCalled();
    expect(socket.send).not.toHaveBeenCalled();
  });

  it("renders the other author name through Untrusted in the blocked reason", () => {
    const store = storeFor();
    store.set(authoringAtom, {
      holder: { ...hello.you, name: "Other\u202Eauthor", away_since: null, frees_at: null },
    });
    renderPanel(store);

    expect(screen.getByText("Other⟨U+202E⟩author holds authoring")).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Stop the team" }).getAttribute("aria-disabled"),
    ).toBe("true");
  });

  it("shows a refused stop as visible text and allows retrying", async () => {
    const store = storeFor();
    const socket = installSocket(store);
    vi.spyOn(window, "confirm").mockReturnValue(true);
    renderPanel(store);

    const button = screen.getByRole("button", { name: "Stop the team" });
    fireEvent.click(button);
    await act(async () => socket.reply("Stop\u202Erefused"));

    expect(screen.getByRole("alert").textContent).toBe("Stop⟨U+202E⟩refused");
    expect(button.getAttribute("aria-disabled")).toBeNull();
  });
});
