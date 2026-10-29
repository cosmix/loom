import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";

import { BriefDialogHost } from "./brief-dialog";
import { snapshotAtom } from "./model";
import { StateLine } from "./state-line";
import { harness, installStartChatLifecycle, session } from "./start-chat-test-kit";
import { StartingBrief } from "./start-chat";

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
