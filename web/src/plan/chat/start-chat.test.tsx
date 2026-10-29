import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";

import brief from "@/plan/api/fixtures/session-brief.json";
import { EmptyOffer } from "@/plan/concept/empty-offer";
import { authoringAtom, serverInfoAtom } from "@/plan/state/atoms";
import { BriefDialog, BriefDialogHost } from "./brief-dialog";
import { sendMessage, startChat } from "./commands";
import { EndChatMenuItem } from "./end-chat";
import { snapshotAtom } from "./model";
import { createOfferStarter } from "./offer-flow";
import { registerAuthoringOffers } from "./offer-handlers";
import { panelCollapsedAtom } from "./prefs";
import { SessionDivider } from "./session-divider";
import { chatSheetOpenAtom } from "./sheet-state";
import {
  disposers,
  harness,
  installStartChatLifecycle,
  offer,
  session,
} from "./start-chat-test-kit";
import { NewChatButton, StartingBrief } from "./start-chat";

installStartChatLifecycle();

it("offer start", async () => {
  const h = harness();
  disposers.push(registerAuthoringOffers((message) => startChat(h.store, message.offer)));
  h.mount(<EmptyOffer offer={offer.id} />);
  fireEvent.click(screen.getByRole("button", { name: offer.label }));
  expect(h.send).toHaveBeenCalledWith(
    expect.objectContaining({
      type: "chat",
      command: "start",
      offer: offer.id,
      text: null,
      attachments: [],
    }),
  );
  h.result();
  await act(async () => {});
});

it("new chat start", async () => {
  const h = harness();
  h.store.set(panelCollapsedAtom, true);
  h.mount(<NewChatButton />);
  fireEvent.click(screen.getByRole("button", { name: "New chat" }));
  expect(h.send).toHaveBeenCalledWith(
    expect.objectContaining({ command: "start", offer: null, text: null, attachments: [] }),
  );
  expect(h.store.get(panelCollapsedAtom)).toBe(true);
  expect(h.store.get(chatSheetOpenAtom)).toBe(false);
  h.result();
  await waitFor(() => expect(h.store.get(chatSheetOpenAtom)).toBe(true));
  expect(h.store.get(panelCollapsedAtom)).toBe(false);
});

it("first message start", async () => {
  const h = harness();
  const pending = sendMessage(h.store, "Help refine this specification");
  expect(h.send).toHaveBeenCalledWith(
    expect.objectContaining({
      command: "send",
      text: "Help refine this specification",
      attachments: [],
    }),
  );
  h.result();
  await expect(pending).resolves.toMatchObject({ ok: true });
  act(() => h.store.set(snapshotAtom, { session, turns: [], queue: [], previous: null }));
  h.mount(<StartingBrief session={session} />);
  expect(await screen.findByText(brief.display_name)).toBeTruthy();
});

it("starting shows the brief view and leaves the panel as it is", async () => {
  const h = harness(session);
  h.store.set(panelCollapsedAtom, true);
  h.mount(
    <>
      <StartingBrief session={session} />
      <BriefDialogHost />
    </>,
  );
  expect(await screen.findByText(brief.display_name)).toBeTruthy();
  expect(screen.getByText(brief.model)).toBeTruthy();
  expect(screen.getByText(brief.effort)).toBeTruthy();
  expect(h.store.get(panelCollapsedAtom)).toBe(true);
  expect(h.store.get(chatSheetOpenAtom)).toBe(false);
  fireEvent.click(screen.getByRole("button", { name: "Brief" }));
  const dialog = await screen.findByRole("dialog", { name: "Brief" });
  expect(
    await within(dialog).findByText(/Write an implementation plan for the guidance engine/),
  ).toBeTruthy();
});

it("brief dialog read-only", async () => {
  const h = harness(session);
  h.mount(<BriefDialog sessionId={session.id} onClose={() => {}} />);
  const dialog = screen.getByRole("dialog", { name: "Brief" });
  expect(
    await within(dialog).findByText(/Write an implementation plan for the guidance engine/),
  ).toBeTruthy();
  expect(within(dialog).queryByRole("textbox")).toBeNull();
  expect(dialog.querySelector("[contenteditable=true]")).toBeNull();
  expect(within(dialog).queryByRole("button", { name: /save|edit/i })).toBeNull();
});

it("new chat waits for the current chat to end", async () => {
  const current = { ...session, state: "idle" as const };
  const h = harness(current);
  h.mount(<NewChatButton />);
  fireEvent.click(screen.getByRole("button", { name: "New chat" }));
  expect(window.confirm).toHaveBeenCalledWith(
    "End the current chat first? It will write its open questions into the documents before it stops.",
  );
  expect(h.send).toHaveBeenCalledWith(expect.objectContaining({ command: "end" }));
  h.result();
  await act(async () => {});
  expect(h.send).toHaveBeenCalledTimes(1);
  act(() =>
    h.store.set(snapshotAtom, {
      session: { ...current, state: "ending" },
      turns: [],
      queue: [],
      previous: null,
    }),
  );
  expect(h.send).toHaveBeenCalledTimes(1);
  act(() =>
    h.store.set(snapshotAtom, {
      session: { ...current, state: "ended" },
      turns: [],
      queue: [],
      previous: null,
    }),
  );
  await waitFor(() => expect(h.send).toHaveBeenCalledTimes(2));
  expect(h.send.mock.calls[1][0]).toMatchObject({ command: "start" });
  h.result(1);
  await act(async () => {});
});

it("end current chat first", async () => {
  const current = { ...session, state: "idle" as const };
  const h = harness(current);
  const starter = createOfferStarter(h.store);
  disposers.push(
    registerAuthoringOffers((message) => starter.start(message.offer)),
    starter.dispose,
  );
  h.mount(<EmptyOffer offer={offer.id} />);
  fireEvent.click(screen.getByRole("button", { name: offer.label }));
  expect(window.confirm).toHaveBeenCalledWith(
    "End the current chat first? It will write its open questions into the documents before it stops.",
  );
  expect(h.send).toHaveBeenCalledWith(expect.objectContaining({ command: "end" }));
  h.result();
  await act(async () => {});
  expect(h.send).toHaveBeenCalledTimes(1);
  act(() =>
    h.store.set(snapshotAtom, {
      session: { ...current, state: "ended" },
      turns: [],
      queue: [],
      previous: null,
    }),
  );
  await waitFor(() => expect(h.send).toHaveBeenCalledTimes(2));
  expect(h.send.mock.calls[1][0]).toMatchObject({ command: "start", offer: offer.id });
  h.result(1);
  await act(async () => {});
});

it("cancelling end current chat keeps it running", () => {
  const h = harness({ ...session, state: "idle" });
  vi.mocked(window.confirm).mockReturnValue(false);
  h.mount(<NewChatButton />);
  fireEvent.click(screen.getByRole("button", { name: "New chat" }));
  expect(h.send).not.toHaveBeenCalled();
});

it("lost race message", async () => {
  const h = harness();
  h.store.set(panelCollapsedAtom, true);
  h.mount(<NewChatButton />);
  fireEvent.click(screen.getByRole("button", { name: "New chat" }));
  act(() =>
    h.store.set(snapshotAtom, {
      session: { ...session, holder: "Alex" },
      turns: [],
      queue: [],
      previous: null,
    }),
  );
  act(() =>
    h.store.set(authoringAtom, {
      holder: { public_id: "winner", name: "Alex", away_since: null, frees_at: null },
    }),
  );
  h.result(0, "Another browser holds authoring (Alex)");
  expect(await screen.findByText("Alex started a chat first")).toBeTruthy();
  expect(h.store.get(panelCollapsedAtom)).toBe(true);
  expect(h.store.get(chatSheetOpenAtom)).toBe(false);
});

it("end chat confirms then ends", async () => {
  const h = harness({ ...session, state: "idle" });
  h.mount(<EndChatMenuItem />);
  vi.mocked(window.confirm).mockReturnValueOnce(false);
  fireEvent.click(screen.getByRole("button", { name: "End chat" }));
  expect(h.send).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "End chat" }));
  expect(window.confirm).toHaveBeenLastCalledWith(
    "End this chat? The session will write its open questions and anything pending into the documents, then stop.",
  );
  expect(h.send).toHaveBeenCalledWith(expect.objectContaining({ command: "end" }));
  h.result();
  await act(async () => {});
});

it("end chat hidden for pressure sessions", () => {
  const h = harness({ ...session, kind: "pressure" });
  h.mount(<EndChatMenuItem />);
  expect(screen.queryByRole("button", { name: "End chat" })).toBeNull();
});

it("viewer has no end chat, brief opens", async () => {
  const h = harness(session);
  h.store.set(authoringAtom, {
    holder: { public_id: "another-browser", name: "Alex", away_since: null, frees_at: null },
  });
  h.mount(
    <>
      <EndChatMenuItem />
      <StartingBrief session={session} />
      <BriefDialogHost />
    </>,
  );
  expect(screen.queryByRole("button", { name: "End chat" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Brief" }));
  const dialog = await screen.findByRole("dialog", { name: "Brief" });
  expect(
    await within(dialog).findByText(/Write an implementation plan for the guidance engine/),
  ).toBeTruthy();
});

it("Chat ended at HH:MM", () => {
  const ended = {
    ...session,
    state: "ended" as const,
    state_since: new Date(2026, 9, 28, 14, 7).toISOString(),
  };
  render(<SessionDivider previous={{ session: ended, turns: [] }} next={null} />);
  expect(screen.getByText("Chat ended at 14:07")).toBeTruthy();
});

it("brief retry stops after 30 tries", async () => {
  vi.useFakeTimers();
  vi.stubGlobal(
    "fetch",
    vi
      .fn()
      .mockImplementation(
        async () => new Response(JSON.stringify({ error: "unknown_session" }), { status: 404 }),
      ),
  );
  const h = harness(session);
  h.mount(<BriefDialog sessionId={session.id} onClose={() => {}} />);
  await act(async () => {});
  await act(async () => {
    await vi.advanceTimersByTimeAsync(30_000);
  });
  expect(fetch).toHaveBeenCalledTimes(30);
  expect(screen.getByText("Brief not available")).toBeTruthy();
  await act(async () => {
    await vi.advanceTimersByTimeAsync(60_000);
  });
  expect(fetch).toHaveBeenCalledTimes(30);
});

it.each(["viewer", "read-only"])("empty offers are text for %s", (mode) => {
  const h = harness();
  disposers.push(registerAuthoringOffers(vi.fn()));
  if (mode === "viewer")
    h.store.set(authoringAtom, {
      holder: { public_id: "another", name: "Alex", away_since: null, frees_at: null },
    });
  else
    h.store.set(
      serverInfoAtom,
      (server) =>
        server && {
          ...server,
          read_only: [{ what: "claude", message: "Missing Claude", fix: "Install Claude" }],
        },
    );
  h.mount(<EmptyOffer offer={offer.id} />);
  expect(screen.getByText(offer.label)).toBeTruthy();
  expect(screen.queryByRole("button")).toBeNull();
});

it("empty offers allow taking free authoring", async () => {
  const h = harness();
  h.store.set(authoringAtom, { holder: null });
  disposers.push(registerAuthoringOffers((message) => startChat(h.store, message.offer)));
  h.mount(<EmptyOffer offer={offer.id} />);
  fireEvent.click(screen.getByRole("button", { name: offer.label }));
  expect(h.send).toHaveBeenCalledWith(
    expect.objectContaining({ command: "start", offer: offer.id }),
  );
  h.result();
  await act(async () => {});
});
