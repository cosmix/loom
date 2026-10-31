import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

import { projectChecklistViewSchema } from "@/plan/api/schema";
import { projectAtom } from "@/plan/state/atoms";
import {
  deliver,
  docServer,
  installDocServer,
  pathOf,
  pathsAt,
  sender,
  sentAt,
  view,
} from "./composer-context-kit";
import { installComposerLifecycle, mount, queue, setup } from "./composer-harness";
import { checklist } from "./context-kit";
import { forgetGiven, lastAttachedAtom } from "./context-attach";
import { draftAtom } from "./drafts";
import { queueReturnedAtom } from "./model";
import { screenAtom } from "./session-home";

installComposerLifecycle();
installDocServer();
afterEach(() => vi.useRealTimers());

/** Presses send with the viewed document's fetch held, so the draft can change during the wait. */
async function pressWhileFetching(h: ReturnType<typeof setup>, text: string) {
  const hook = sender(h);
  let release = () => {};
  docServer.gate = new Promise<void>((resolve) => (release = resolve));
  act(() => h.store.set(draftAtom, { text, attachments: [] }));
  await act(async () => {
    void hook.result.current.send();
  });
  expect(hook.result.current.pending).toBe(true);
  /** Lets the fetch answer; with `answered`, also answers the command the press sends. */
  const finish = async (answered = false) => {
    await act(async () => release());
    if (answered) {
      await waitFor(() => expect(h.sent).toHaveLength(1));
      await h.result();
    }
    await waitFor(() => expect(hook.result.current.pending).toBe(false));
  };
  return { hook, finish };
}

it("a box cleared during the fetch sends nothing, records nothing and leaves the draft alone", async () => {
  const h = setup("idle");
  view(h, "05");
  const { hook, finish } = await pressWhileFetching(h, "Hello");
  act(() => h.store.set(draftAtom, { text: "", attachments: [] }));

  await finish();

  expect(h.sent).toEqual([]);
  expect(h.store.get(lastAttachedAtom)).toBeNull();
  expect(h.store.get(draftAtom)).toEqual({ text: "", attachments: [] });
  expect(hook.result.current.error).toBeNull();
});

it("the text edited during the fetch is what is sent", async () => {
  const h = setup("idle");
  view(h, "05");
  const { finish } = await pressWhileFetching(h, "Before");
  act(() => h.store.set(draftAtom, { text: "After", attachments: [] }));

  await finish(true);

  expect(sentAt(h, 0).text).toBe("After");
});

it("a document that would push the message over the size limit is dropped, not the send", async () => {
  const h = setup("idle");
  view(h, "05");
  docServer.text = "y".repeat(3_000);
  const hook = sender(h);

  // 99,000 bytes of text alone pass the 100,000-byte limit; the document's quote does not fit.
  await deliver(h, hook, "a".repeat(99_000));

  expect(pathsAt(h, 0)).toEqual([]);
  expect(h.store.get(lastAttachedAtom)).toBeNull();
  expect(hook.result.current.error).toBeNull();
});

it("a chat started by the press belongs to the screen the press was made on", async () => {
  const h = setup(null);
  view(h, "05");
  h.store.set(
    projectAtom,
    projectChecklistViewSchema.parse({
      checklists: [checklist({ feature: "a" }), checklist({ feature: "b" })],
      standalone: [],
      moves: [],
    }),
  );
  act(() => h.store.set(screenAtom, { feature: "a", plan: null }));
  const { finish } = await pressWhileFetching(h, "Question");
  act(() => h.store.set(screenAtom, { feature: "b", plan: null }));

  await finish(true);

  expect(sentAt(h, 0)).toMatchObject({ command: "start", feature: "a" });
});

it("a fetch that hangs is given up after 4 seconds and the message goes without the document", async () => {
  vi.useFakeTimers();
  const h = setup("idle");
  view(h, "05");
  const hook = sender(h);
  docServer.gate = new Promise<void>(() => {});
  act(() => h.store.set(draftAtom, { text: "Hung", attachments: [] }));
  await act(async () => {
    void hook.result.current.send();
  });
  expect(hook.result.current.pending).toBe(true);

  await act(async () => {
    await vi.advanceTimersByTimeAsync(3_999);
  });
  expect(h.sent).toHaveLength(0);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(1);
  });
  expect(h.sent).toHaveLength(1);
  expect(pathsAt(h, 0)).toEqual([]);

  await h.result();
  expect(hook.result.current.pending).toBe(false);
  expect(h.store.get(lastAttachedAtom)).toBeNull();
});

it("after the chat forgets what it was given, the next send attaches the document again", async () => {
  const h = setup("idle");
  view(h, "05");
  const hook = sender(h);
  await deliver(h, hook, "First");
  await deliver(h, hook, "Second");
  expect(pathsAt(h, 1)).toEqual([]);

  forgetGiven(h.store);
  await deliver(h, hook, "Third");

  expect(pathsAt(h, 2)).toEqual([pathOf("05")]);
});

it.each(["Remove", "Edit"])(
  "%s on a queued message makes the chat forget the document",
  async (label) => {
    const h = setup("working");
    queue(h.store, 1);
    act(() => h.store.set(lastAttachedAtom, { path: pathOf("05"), session: "s" }));
    mount(h.store);

    fireEvent.click(screen.getByRole("button", { name: label }));
    expect(h.store.get(lastAttachedAtom)).not.toBeNull();
    await h.result();

    await waitFor(() => expect(h.store.get(lastAttachedAtom)).toBeNull());
  },
);

it("a queue the server returns makes the chat forget the document", async () => {
  const h = setup("working");
  act(() => h.store.set(lastAttachedAtom, { path: pathOf("05"), session: "s" }));
  mount(h.store);

  act(() => h.store.set(queueReturnedAtom, ["Returned words"]));

  await waitFor(() => expect(h.store.get(lastAttachedAtom)).toBeNull());
  expect(h.store.get(draftAtom).text).toBe("Returned words");
});
