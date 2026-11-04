import { cleanup, createEvent, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import hello from "@/plan/api/fixtures/hello.json";
import { pendingAttachmentsAtom, uploadStatusAtom } from "@/plan/attachments";
import { authoringAtom, connectionAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";
import { useFileIntake } from "./composer-hooks";
import { draftAtom, MAX_ATTACHMENTS } from "./drafts";

function Box() {
  const { dragging, ...handlers } = useFileIntake();
  return <div data-testid="box" data-dragging={dragging} {...handlers} />;
}

function holderStore() {
  const store = createStore();
  store.set(serverInfoAtom, { ...hello.server, read_only: [] });
  store.set(youAtom, hello.you);
  store.set(authoringAtom, { holder: { ...hello.you, away_since: null, frees_at: null } });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: null });
  return store;
}

function mount(store = holderStore()) {
  render(
    <Provider store={store}>
      <Box />
    </Provider>,
  );
  return { store, box: screen.getByTestId("box") };
}

const png = () => new File(["png"], "sketch.png", { type: "image/png" });
const stored = {
  id: "0123456789abcdef",
  name: "sketch.png",
  media_type: "image/png",
  bytes: 3,
};

function answer(status = 200, body: unknown = stored) {
  const fetchMock = vi
    .fn<typeof fetch>()
    .mockImplementation(async () => new Response(JSON.stringify(body), { status }));
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

beforeEach(() => {
  vi.stubGlobal(
    "URL",
    Object.assign(URL, { createObjectURL: () => "blob:x", revokeObjectURL: () => {} }),
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("a dropped file is uploaded and becomes a file chip, and the target shows while dragging", async () => {
  const fetchMock = answer();
  const { store, box } = mount();
  const dataTransfer = { files: [png()], types: ["Files"] };

  fireEvent.dragOver(box, { dataTransfer });
  expect(box.getAttribute("data-dragging")).toBe("true");
  fireEvent.drop(box, { dataTransfer });

  expect(box.getAttribute("data-dragging")).toBe("false");
  await waitFor(() =>
    expect(store.get(pendingAttachmentsAtom)).toEqual([{ kind: "file", ...stored }]),
  );
  expect(String(fetchMock.mock.calls[0]![0])).toBe("/api/attachments?name=sketch.png");
  expect(store.get(uploadStatusAtom)).toEqual({ busy: 0, problems: [] });
});

function leave(box: HTMLElement, relatedTarget: Node) {
  // jsdom has no DragEvent, so the related target is set on the event itself.
  const event = createEvent.dragLeave(box, { dataTransfer: { files: [], types: ["Files"] } });
  Object.defineProperty(event, "relatedTarget", { value: relatedTarget });
  fireEvent(box, event);
}

it("leaving the box ends the drag, but moving onto a child does not", () => {
  const { box } = mount();
  const child = document.createElement("span");
  box.append(child);

  fireEvent.dragOver(box, { dataTransfer: { files: [], types: ["Files"] } });
  leave(box, child);
  expect(box.getAttribute("data-dragging")).toBe("true");
  leave(box, document.body);
  expect(box.getAttribute("data-dragging")).toBe("false");
});

it("a dragged piece of text is left alone", () => {
  const fetchMock = answer();
  const { box } = mount();

  const notPrevented = fireEvent.dragOver(box, {
    dataTransfer: { files: [], types: ["text/plain"] },
  });

  expect(notPrevented).toBe(true);
  expect(box.getAttribute("data-dragging")).toBe("false");
  expect(fetchMock).not.toHaveBeenCalled();
});

it("a pasted image is uploaded, and a text paste is not intercepted", async () => {
  const fetchMock = answer();
  const { store, box } = mount();

  const textPaste = fireEvent.paste(box, {
    clipboardData: { files: [], getData: () => "some words" },
  });
  expect(textPaste).toBe(true);
  expect(fetchMock).not.toHaveBeenCalled();

  const imagePaste = fireEvent.paste(box, {
    clipboardData: { files: [png()], getData: () => "" },
  });
  expect(imagePaste).toBe(false);
  await waitFor(() => expect(store.get(pendingAttachmentsAtom)).toHaveLength(1));
});

it("a paste that carries both text and a picture keeps the text and uploads nothing", () => {
  const fetchMock = answer();
  const { box } = mount();

  const mixed = fireEvent.paste(box, {
    clipboardData: { files: [png()], getData: () => "cell A1\tcell B1" },
  });

  expect(mixed).toBe(true);
  expect(fetchMock).not.toHaveBeenCalled();
});

it("only as many files as fit under the cap are uploaded, and the rest are named", async () => {
  const fetchMock = answer();
  const { store, box } = mount();
  const room = 2;
  store.set(draftAtom, {
    text: "",
    attachments: Array.from({ length: MAX_ATTACHMENTS - room }, (_, index) => ({
      kind: "file" as const,
      id: `file-${index}`,
      name: `f${index}.png`,
      media_type: "image/png",
      bytes: 1,
    })),
  });
  const files = ["a", "b", "c"].map(
    (name) => new File([name], `${name}.png`, { type: "image/png" }),
  );

  fireEvent.drop(box, { dataTransfer: { files, types: ["Files"] } });

  await waitFor(() => expect(store.get(uploadStatusAtom).busy).toBe(0));
  expect(fetchMock).toHaveBeenCalledTimes(room);
  expect(store.get(uploadStatusAtom).problems).toEqual([
    "c.png was not attached: at most 10 attachments per message.",
  ]);
});

it("shows the server's reason for a refused upload and adds no chip", async () => {
  answer(415, { error: "unsupported", message: "Only images, PDF and text are accepted" });
  const { store, box } = mount();

  fireEvent.drop(box, { dataTransfer: { files: [png()], types: ["Files"] } });

  await waitFor(() => expect(store.get(uploadStatusAtom).problems).toHaveLength(1));
  expect(store.get(uploadStatusAtom).problems[0]).toBe(
    "sketch.png: Only images, PDF and text are accepted",
  );
  expect(store.get(pendingAttachmentsAtom)).toEqual([]);
});

it("uploads nothing when this browser cannot author", () => {
  const fetchMock = answer();
  const store = holderStore();
  store.set(authoringAtom, { holder: null });
  store.set(serverInfoAtom, {
    ...hello.server,
    read_only: [{ what: "claude", message: "Claude is not installed", fix: "Install it" }],
  });
  const { box } = mount(store);

  fireEvent.drop(box, { dataTransfer: { files: [png()], types: ["Files"] } });

  expect(fetchMock).not.toHaveBeenCalled();
  expect(store.get(uploadStatusAtom).problems).toHaveLength(1);
});
