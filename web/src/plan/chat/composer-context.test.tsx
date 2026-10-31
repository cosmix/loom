import { act, waitFor } from "@testing-library/react";
import { expect, it } from "vitest";

import { docAttachment } from "@/plan/attachments/doc-attachment";
import { box, initial, installComposerLifecycle, mount, setup, source } from "./composer-harness";
import {
  BODY,
  deliver,
  docServer,
  installDocServer,
  newSession,
  pathOf,
  pathsAt,
  sender,
  sentAt,
  view,
} from "./composer-context-kit";
import { bindAttachedSession } from "./context-attach";
import { draftAtom } from "./drafts";
import { snapshotAtom } from "./model";
import { screenAtom } from "./session-home";
import { contextDismissedAtom, viewedDocumentAtom } from "./viewed-document";

installComposerLifecycle();
installDocServer();

it("a first message carries the viewed document, as a source attachment with its text", async () => {
  const h = setup("idle");
  view(h, "05");
  await deliver(h, sender(h));
  expect(sentAt(h, 0).attachments).toHaveLength(1);
  expect(sentAt(h, 0).attachments[0]).toMatchObject({ kind: "source", path: pathOf("05") });
  expect(sentAt(h, 0).attachments[0].quote).toContain(BODY);
});

it("a passage the user selected stays first and the viewed document follows it", async () => {
  const h = setup("idle");
  view(h, "05");
  act(() => h.store.set(draftAtom, { text: "", attachments: [source()] }));
  await deliver(h, sender(h));
  expect(pathsAt(h, 0)).toEqual(["doc/product/note-0.md", pathOf("05")]);
});

it("the same document is not sent again in the same session", async () => {
  const h = setup("idle");
  view(h, "05");
  const hook = sender(h);
  await deliver(h, hook, "First");
  await deliver(h, hook, "Second");
  expect(pathsAt(h, 1)).toEqual([]);
  expect(docServer.asked).toEqual([pathOf("05")]);
});

it("a message after moving to another document carries it, and returning carries the first again", async () => {
  const h = setup("idle");
  const hook = sender(h);
  view(h, "05");
  await deliver(h, hook);
  view(h, "07");
  await deliver(h, hook);
  expect(pathsAt(h, 1)).toEqual([pathOf("07")]);
  await deliver(h, hook);
  expect(pathsAt(h, 2)).toEqual([]);
  view(h, "05");
  await deliver(h, hook);
  expect(pathsAt(h, 3)).toEqual([pathOf("05")]);
});

it("a new session starts with nothing attached", async () => {
  const h = setup("idle");
  view(h, "05");
  const hook = sender(h);
  await deliver(h, hook);
  newSession(h, "another-session");
  await deliver(h, hook);
  expect(pathsAt(h, 1)).toEqual([pathOf("05")]);
});

it("a chat started by the message is told once, and a fresh chat after it is told again", async () => {
  const h = setup(null);
  const unbind = bindAttachedSession(h.store);
  view(h, "05");
  const hook = sender(h);
  await deliver(h, hook, "Opening");
  expect(sentAt(h, 0).command).toBe("start");
  expect(pathsAt(h, 0)).toEqual([pathOf("05")]);

  // The snapshot that reports the new chat arrives after the start's result.
  act(() =>
    h.store.set(snapshotAtom, (s) => ({ ...s, session: { ...initial.session!, state: "idle" } })),
  );
  await deliver(h, hook, "Follow-up");
  expect(sentAt(h, 1).command).toBe("send");
  expect(pathsAt(h, 1)).toEqual([]);

  // The chat ends, and the next message starts another one.
  act(() => h.store.set(snapshotAtom, (s) => ({ ...s, session: null })));
  await deliver(h, hook, "Next chat");
  expect(sentAt(h, 2).command).toBe("start");
  expect(pathsAt(h, 2)).toEqual([pathOf("05")]);
  unbind();
});

it("a dismissed context is not fetched or sent", async () => {
  const h = setup("idle");
  view(h, "05");
  act(() => h.store.set(contextDismissedAtom, { path: pathOf("05") }));
  await deliver(h, sender(h));
  expect(pathsAt(h, 0)).toEqual([]);
  expect(docServer.asked).toEqual([]);
});

it("a page with no source file adds nothing and fetches nothing", async () => {
  const h = setup("idle");
  act(() => h.store.set(viewedDocumentAtom, { kind: "run", path: null, name: "Run" }));
  await deliver(h, sender(h));
  expect(pathsAt(h, 0)).toEqual([]);
  expect(docServer.asked).toEqual([]);
});

it("a document that cannot be fetched is left out, quietly, and the next message tries again", async () => {
  const h = setup("idle");
  view(h, "05");
  const hook = sender(h);
  docServer.missing = true;
  await deliver(h, hook, "First");
  expect(pathsAt(h, 0)).toEqual([]);
  expect(sentAt(h, 0).command).toBe("send");
  expect(hook.result.current.error).toBeNull();
  expect(h.store.get(draftAtom).text).toBe("");

  docServer.missing = false;
  await deliver(h, hook, "Second");
  expect(pathsAt(h, 1)).toEqual([pathOf("05")]);
});

it("pressing Send again while the document is being fetched sends once", async () => {
  const h = setup("idle");
  view(h, "05");
  const hook = sender(h);
  let release = () => {};
  docServer.gate = new Promise<void>((resolve) => (release = resolve));
  act(() => h.store.set(draftAtom, { text: "Once", attachments: [] }));
  await act(async () => {
    void hook.result.current.send();
    void hook.result.current.send();
  });
  expect(h.sent).toHaveLength(0);
  // The draft is still whole while nothing has been sent.
  expect(h.store.get(draftAtom).text).toBe("Once");
  await act(async () => release());
  await waitFor(() => expect(h.sent).toHaveLength(1));
  await h.result();
  await waitFor(() => expect(hook.result.current.pending).toBe(false));
  expect(h.sent).toHaveLength(1);
  expect(docServer.asked).toEqual([pathOf("05")]);
});

it("a refused message keeps the draft and its passage, and does not count the document as told", async () => {
  const h = setup("idle");
  view(h, "05");
  const hook = sender(h);
  act(() => h.store.set(draftAtom, { text: "Try", attachments: [source()] }));
  await act(async () => {
    void hook.result.current.send();
  });
  await waitFor(() => expect(h.sent).toHaveLength(1));
  await h.result("refused");
  await waitFor(() => expect(hook.result.current.pending).toBe(false));
  expect(hook.result.current.error).toBe("refused");
  expect(h.store.get(draftAtom)).toMatchObject({ text: "Try", attachments: [source()] });

  await deliver(h, hook, "Try");
  expect(pathsAt(h, 1)).toEqual(["doc/product/note-0.md", pathOf("05")]);
});

it("a document already attached from Discuss is not attached twice, and counts as told", async () => {
  const h = setup("idle");
  view(h, "05");
  const discussed = docAttachment({
    path: pathOf("05"),
    text: BODY,
    hash: "sha256:abc",
  });
  act(() => h.store.set(draftAtom, { text: "", attachments: [discussed] }));
  const hook = sender(h);
  await deliver(h, hook, "First");
  expect(pathsAt(h, 0)).toEqual([pathOf("05")]);
  await deliver(h, hook, "Second");
  expect(pathsAt(h, 1)).toEqual([]);
  expect(docServer.asked).toEqual([]);
});

it("the composer invites a question about the feature only where the screen has one", () => {
  const h = setup("idle");
  mount(h.store);
  // The kickoff screen has no feature.
  expect(box().placeholder).toBe("");

  act(() => h.store.set(screenAtom, { feature: "atlas", plan: null }));
  expect(box().placeholder).toBe("Ask about this feature");

  view(h, "05");
  expect(box().placeholder).toBe("Ask about section_05, or say what to change");
  act(() => h.store.set(contextDismissedAtom, { path: pathOf("05") }));
  expect(box().placeholder).toBe("Ask about this feature");
});
