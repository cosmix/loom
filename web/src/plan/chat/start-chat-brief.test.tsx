import { act, fireEvent, renderHook, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";

import helloFixture from "@/plan/api/fixtures/hello.json";
import brief from "@/plan/api/fixtures/session-brief.json";
import { checklistViewSchema } from "@/plan/api/schema";
import { projectAtom } from "@/plan/state/atoms";
import { BriefDialog, BriefDialogHost } from "./brief-dialog";
import { snapshotAtom } from "./model";
import { screenAtom } from "./session-home";
import { StateLine } from "./state-line";
import { harness, installStartChatLifecycle, session } from "./start-chat-test-kit";
import { NewChatButton, StartingBrief } from "./start-chat";
import { useSessionBrief } from "./use-session-brief";

installStartChatLifecycle();

it("closing the Brief returns focus to the link while the Starting row is still there", async () => {
  const h = harness(session);
  h.mount(
    <>
      <h2 tabIndex={-1} data-chat-heading>
        Chat
      </h2>
      <StartingBrief session={session} />
      <BriefDialogHost />
    </>,
  );
  const link = screen.getByRole("button", { name: "Brief" });
  link.focus();
  fireEvent.click(link);
  fireEvent.keyDown(await screen.findByRole("dialog", { name: "Brief" }), { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  await waitFor(() => expect(document.activeElement).toBe(link));
});

it("an open Brief outlives the Starting row, and closing it lands on the panel heading", async () => {
  const h = harness(session);
  h.mount(
    <>
      <h2 tabIndex={-1} data-chat-heading>
        Chat
      </h2>
      <StateLine />
      <BriefDialogHost />
    </>,
  );
  const link = screen.getByRole("button", { name: "Brief" });
  link.focus();
  fireEvent.click(link);
  const dialog = await screen.findByRole("dialog", { name: "Brief" });
  act(() =>
    h.store.set(snapshotAtom, {
      session: { ...session, state: "working" },
      turns: [],
      queue: [],
      previous: null,
    }),
  );
  // The session started: the row and its link are gone, the dialog being read is not.
  expect(screen.queryByRole("button", { name: "Brief", hidden: true })).toBeNull();
  expect(screen.getByRole("dialog", { name: "Brief" })).toBe(dialog);
  fireEvent.keyDown(dialog, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  await waitFor(() =>
    expect(document.activeElement).toBe(screen.getByRole("heading", { name: "Chat" })),
  );
});

it("the brief loading state is a line of text, not a block", () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(() => new Promise<Response>(() => {})),
  );
  const h = harness(session);
  const { container } = h.mount(<StartingBrief session={session} />);
  expect(screen.getByRole("status").textContent).toBe("Loading brief…");
  expect(container.querySelector('[class*="h-24"]')).toBeNull();
});

it.each([
  ["a pressure test", { ...session, kind: "pressure" }],
  ["a chat past its start", { ...session, state: "idle" as const }],
  ["a session that is no longer the chat", null],
])("a 404 for %s is final: one request and no polling", async (_name, current) => {
  vi.useFakeTimers();
  vi.stubGlobal(
    "fetch",
    vi
      .fn()
      .mockImplementation(
        async () => new Response(JSON.stringify({ error: "unknown_session" }), { status: 404 }),
      ),
  );
  const h = harness(current);
  h.mount(<BriefDialog sessionId={session.id} onClose={() => {}} />);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(60_000);
  });
  expect(fetch).toHaveBeenCalledTimes(1);
  expect(screen.getByText("Brief not available")).toBeTruthy();
});

it("a new session does not paint with the previous session's brief", async () => {
  const fetchMock = vi.fn(async () => new Response(JSON.stringify(brief)));
  vi.stubGlobal("fetch", fetchMock);
  const { result, rerender } = renderHook(({ id }) => useSessionBrief(id), {
    initialProps: { id: "first" },
  });
  await waitFor(() => expect(result.current.status).toBe("ready"));
  fetchMock.mockImplementationOnce(() => new Promise<Response>(() => {}));
  rerender({ id: "second" });
  expect(result.current.status).toBe("loading");
});

it("a pressure test's start row has no brief to load or fail", () => {
  const fetchMock = vi.fn(() => new Promise<Response>(() => {}));
  vi.stubGlobal("fetch", fetchMock);
  const h = harness();
  h.mount(<StartingBrief session={{ ...session, kind: "pressure" }} />);
  expect(screen.queryByRole("button", { name: "Brief" })).toBeNull();
  expect(screen.queryByRole("status")).toBeNull();
  expect(fetchMock).not.toHaveBeenCalled();
});

it("New chat starts for the screen it was clicked on, whatever the end wait lasts", async () => {
  const current = { ...session, state: "idle" as const };
  const h = harness(current);
  const checklist = checklistViewSchema.parse(helloFixture.checklists[0]);
  const features = ["alpha", "beta"].map((feature) => ({ ...checklist, feature }));
  h.store.set(projectAtom, { checklists: features, standalone: [], moves: [] });
  h.store.set(screenAtom, { feature: "alpha", plan: null });
  h.mount(<NewChatButton />);
  fireEvent.click(screen.getByRole("button", { name: "New chat" }));
  expect(h.send.mock.calls[0]![0]).toMatchObject({ command: "end" });
  h.result();
  await act(async () => {});
  act(() => {
    h.store.set(screenAtom, { feature: "beta", plan: null });
    h.store.set(snapshotAtom, {
      session: { ...current, state: "ended" },
      turns: [],
      queue: [],
      previous: null,
    });
  });
  await waitFor(() => expect(h.send).toHaveBeenCalledTimes(2));
  expect(h.send.mock.calls[1]![0]).toMatchObject({ command: "start", feature: "alpha" });
});
