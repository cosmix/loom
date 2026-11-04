import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { expect, it, vi } from "vitest";

import { snapshotAtom } from "@/plan/chat/model";
import { panelCollapsedAtom } from "@/plan/chat/prefs";
import { screenAtom } from "@/plan/chat/session-home";
import { chatSheetOpenAtom } from "@/plan/chat/sheet-state";
import { harness, installStartChatLifecycle, session } from "@/plan/chat/start-chat-test-kit";
import type { StarterDocument } from "@/plan/new/document-starter";
import { EmptyKickoff } from "@/plan/new/empty-kickoff";
import { kickoffSessionAtom } from "@/plan/new/kickoff-state";
import { authoringAtom, serverInfoAtom } from "@/plan/state/atoms";

const { document } = vi.hoisted(() => ({
  document: {
    label: "My notes",
    attachment: {
      kind: "source",
      path: "kickoff/notes.md",
      anchor: "",
      lines: null,
      stage: null,
      summary_sentence: null,
      rests_on: [],
      quote: "A place to collect project ideas.",
      selected_hash: null,
    },
  } satisfies StarterDocument,
}));

// Document selection has its own tests; this boundary delivers its picked passage to the screen.
vi.mock("./document-starter", () => ({
  DocumentStarter: ({
    onPick,
    children,
  }: {
    onPick: (document: StarterDocument) => void;
    children: ReactNode;
  }) => (
    <button type="button" onClick={() => onPick(document)}>
      {children}
    </button>
  ),
}));

installStartChatLifecycle();

function mount(small = false, current: typeof session | null = null) {
  const h = harness(current);
  return { ...h, ...h.mount(<EmptyKickoff small={small} />) };
}

const radio = (name: string) => screen.getByRole("radio", { name }) as HTMLInputElement;

function composer() {
  return screen.getByRole("textbox", { name: "What do you want to build?" }) as HTMLTextAreaElement;
}

function submitButton() {
  return screen.getByRole("button", { name: /^(Start the conversation|Try again)$/ });
}

function enterText(text = "Build a project notebook") {
  fireEvent.change(composer(), { target: { value: text } });
}

function showSession(h: ReturnType<typeof harness>, current = session) {
  act(() => h.store.set(snapshotAtom, { session: current, turns: [], queue: [], previous: null }));
}

it("renders the invitation and all conversation starters", () => {
  mount();

  expect(screen.getByRole("heading", { name: "What do you want to build?" })).toBeTruthy();
  expect(screen.getByRole("img")).toBeTruthy();
  expect(screen.getByText(/Describe it the way you would to a colleague/)).toBeTruthy();
  expect(screen.getByRole("button", { name: "A new feature for this project" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "A small fix or improvement" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Grow standalone plans into a feature" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Start from a document I have" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Attach notes or a document" })).toBeTruthy();
  expect(submitButton().textContent).toBe("Start the conversation");
  expect(screen.getByText("Nothing is written until you say so.")).toBeTruthy();
});

it.each([
  ["A new feature for this project", "I want to build a new feature: "],
  ["A small fix or improvement", "I want to fix or improve: "],
  [
    "Grow standalone plans into a feature",
    "I have standalone plans that belong together and want to grow them into a feature.",
  ],
])("fills the composer from %s", (label, text) => {
  mount();

  fireEvent.click(screen.getByRole("button", { name: label }));

  expect(composer().value).toBe(text);
  expect(window.document.activeElement).toBe(composer());
});

it("starts kickoff with the draft and selected document and waits for the session snapshot", async () => {
  const h = mount();
  h.store.set(panelCollapsedAtom, true);
  enterText();
  fireEvent.click(screen.getByRole("button", { name: "Attach notes or a document" }));
  expect(screen.getByRole("listitem").textContent).toContain("Attached: My notes");

  fireEvent.click(submitButton());

  expect(h.send).toHaveBeenCalledWith({
    type: "chat",
    command: "start",
    client_id: expect.any(String),
    offer: "kickoff",
    text: "Build a project notebook",
    attachments: [document.attachment],
  });
  h.result();
  await act(async () => {});
  expect(h.store.get(kickoffSessionAtom)).toBeNull();
  showSession(h);
  await waitFor(() => expect(h.store.get(kickoffSessionAtom)).toBe(session.id));
  expect(h.store.get(panelCollapsedAtom)).toBe(true);
  expect(h.store.get(chatSheetOpenAtom)).toBe(false);
});

it("captures a snapshot that arrived before the start acknowledgement", async () => {
  const h = mount();
  enterText();
  fireEvent.click(submitButton());
  showSession(h);
  expect(h.store.get(kickoffSessionAtom)).toBeNull();

  h.result();

  await waitFor(() => expect(h.store.get(kickoffSessionAtom)).toBe(session.id));
});

it("uses the small kickoff offer after choosing the small starter, shown in the radios", () => {
  const h = mount();
  fireEvent.click(screen.getByRole("button", { name: "A small fix or improvement" }));
  expect(radio("Standalone plan").checked).toBe(true);
  fireEvent.click(submitButton());
  expect(h.send).toHaveBeenCalledWith(
    expect.objectContaining({ offer: "kickoff-small", text: "I want to fix or improve: " }),
  );
});

it("uses the small kickoff offer when the Standalone plan radio is chosen", () => {
  const h = mount();
  fireEvent.click(radio("Standalone plan"));
  enterText();
  fireEvent.click(submitButton());
  expect(h.send).toHaveBeenCalledWith(expect.objectContaining({ offer: "kickoff-small" }));
});

it("switches back to feature kickoff when choosing the feature starter", () => {
  const h = mount(true);
  expect(radio("Standalone plan").checked).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "A new feature for this project" }));
  expect(radio("Feature").checked).toBe(true);
  fireEvent.click(submitButton());
  expect(h.send).toHaveBeenCalledWith(expect.objectContaining({ offer: "kickoff" }));
});

it("allows an attachment to start without draft text", () => {
  const h = mount();
  fireEvent.click(screen.getByRole("button", { name: "Attach notes or a document" }));

  fireEvent.click(submitButton());

  expect(h.send).toHaveBeenCalledWith(
    expect.objectContaining({ text: "", attachments: [document.attachment] }),
  );
});

it("removes an attached document before sending", () => {
  const h = mount();
  fireEvent.click(screen.getByRole("button", { name: "Attach notes or a document" }));
  fireEvent.click(within(screen.getByRole("listitem")).getByRole("button", { name: "Remove" }));

  fireEvent.click(submitButton());

  expect(screen.queryByRole("list", { name: "Attached documents" })).toBeNull();
  expect(submitButton().getAttribute("aria-disabled")).toBe("true");
  expect(h.send).not.toHaveBeenCalled();
});

it("attaches a document from the starter pill as well", () => {
  const h = mount();
  fireEvent.click(screen.getByRole("button", { name: "Start from a document I have" }));
  expect(screen.getByRole("listitem").textContent).toContain("Attached: My notes");

  fireEvent.click(submitButton());

  expect(h.send).toHaveBeenCalledWith(
    expect.objectContaining({ text: "", attachments: [document.attachment] }),
  );
});

it.each(["", "  \n  "])("blocks an empty kickoff draft %j", (text) => {
  const h = mount();
  enterText(text);

  fireEvent.click(submitButton());
  fireEvent.keyDown(composer(), { key: "Enter" });

  expect(submitButton().getAttribute("aria-disabled")).toBe("true");
  expect(screen.getByText("Describe what you want to build or attach a document")).toBeTruthy();
  expect(h.send).not.toHaveBeenCalled();
});

it("sends on Enter while leaving Shift Enter and composition to the textarea", () => {
  const h = mount();
  enterText();

  expect(fireEvent.keyDown(composer(), { key: "Enter", shiftKey: true })).toBe(true);
  expect(fireEvent.keyDown(composer(), { key: "Enter", isComposing: true })).toBe(true);
  expect(h.send).not.toHaveBeenCalled();
  expect(fireEvent.keyDown(composer(), { key: "Enter" })).toBe(false);

  expect(h.send).toHaveBeenCalledOnce();
});

it("shows the read-only reason and blocks a populated draft", () => {
  const h = mount();
  enterText();
  const server = h.store.get(serverInfoAtom);
  if (!server) throw new Error("Expected the server fixture");
  act(() => {
    h.store.set(authoringAtom, { holder: null });
    h.store.set(serverInfoAtom, {
      ...server,
      read_only: [{ what: "tmux", message: "tmux is unavailable", fix: "Install tmux" }],
    });
  });

  fireEvent.click(submitButton());
  fireEvent.keyDown(composer(), { key: "Enter" });

  expect(submitButton().getAttribute("aria-disabled")).toBe("true");
  expect(composer().disabled).toBe(true);
  expect(screen.getAllByText("Read-only")).toHaveLength(1);
  expect(screen.queryByText("Authoring is unavailable")).toBeNull();
  expect(h.send).not.toHaveBeenCalled();
});

it("refuses without asking while a feature's chat runs elsewhere, and starts once it ended", async () => {
  const current = { ...session, state: "idle" as const, feature: "alpha" };
  const h = mount(false, current);
  act(() => h.store.set(screenAtom, { feature: null, plan: null }));
  enterText();

  fireEvent.click(submitButton());

  expect(window.confirm).not.toHaveBeenCalled();
  expect(h.send).not.toHaveBeenCalled();
  expect(screen.getByRole("status").textContent).toBe("Finish the session in alpha first.");
  showSession(h, { ...current, state: "ended" });
  fireEvent.click(submitButton());
  await waitFor(() => expect(h.send).toHaveBeenCalledTimes(1));
  expect(h.send.mock.calls[0][0]).toMatchObject({ command: "start", offer: "kickoff" });
  h.result(0);
  await act(async () => {});
  expect(h.store.get(kickoffSessionAtom)).toBeNull();
  showSession(h, { ...session, id: "new-kickoff" });
  await waitFor(() => expect(h.store.get(kickoffSessionAtom)).toBe("new-kickoff"));
});

it("refuses a project-wide chat too: this screen does not show it either", () => {
  const h = mount(false, session);
  act(() => h.store.set(screenAtom, { feature: null, plan: null }));
  enterText();
  fireEvent.click(submitButton());
  expect(window.confirm).not.toHaveBeenCalled();
  expect(h.send).not.toHaveBeenCalled();
  expect(h.store.get(kickoffSessionAtom)).toBeNull();
  expect(screen.getByRole("status").textContent).toBe("Finish the open chat first.");
  expect(composer().value).toBe("Build a project notebook");
});

it("preserves the draft and renders a start failure safely", async () => {
  const h = mount();
  enterText();
  fireEvent.click(submitButton());

  h.result(0, "Cannot start\u202e");

  expect((await screen.findByRole("status")).textContent).toBe("Cannot start⟨U+202E⟩");
  expect(composer().value).toBe("Build a project notebook");
  expect(h.store.get(kickoffSessionAtom)).toBeNull();
  expect(submitButton().getAttribute("aria-disabled")).toBeNull();
});

it("blocks duplicate starts while waiting for a session", async () => {
  const h = mount();
  enterText();
  fireEvent.click(submitButton());
  h.result();
  await act(async () => {});

  fireEvent.click(submitButton());
  fireEvent.keyDown(composer(), { key: "Enter" });

  expect(h.send).toHaveBeenCalledOnce();
});

it("shows a retryable error after five seconds without a session", async () => {
  vi.useFakeTimers();
  const h = mount();
  h.store.set(kickoffSessionAtom, "previous-kickoff");
  enterText();
  fireEvent.click(submitButton());
  h.result();
  await act(async () => {});

  await act(async () => {
    vi.advanceTimersByTime(4999);
  });
  expect(screen.queryByRole("status")).toBeNull();
  expect(submitButton().getAttribute("aria-disabled")).toBe("true");

  await act(async () => {
    vi.advanceTimersByTime(1);
  });

  expect(h.store.get(kickoffSessionAtom)).toBeNull();
  expect(screen.getByRole("status").textContent).toBe(
    "The conversation did not start. Please try again.",
  );
  const retry = screen.getByRole("button", { name: "Try again" });
  expect(retry.getAttribute("aria-disabled")).toBeNull();
  expect(composer().value).toBe("Build a project notebook");

  fireEvent.click(retry);
  expect(h.send).toHaveBeenCalledTimes(2);
  expect(screen.queryByRole("status")).toBeNull();
  h.result(1);
  showSession(h, { ...session, id: "retry-kickoff" });
  await act(async () => {});
  expect(h.store.get(kickoffSessionAtom)).toBe("retry-kickoff");
});

it("links a late session after timeout, clears the error and keeps it on retry", async () => {
  vi.useFakeTimers();
  const h = mount();
  enterText();
  fireEvent.click(submitButton());
  h.result();
  await act(async () => {});
  await act(async () => {
    vi.advanceTimersByTime(5000);
  });
  const retry = screen.getByRole("button", { name: "Try again" });
  expect(screen.getByRole("status").textContent).toBe(
    "The conversation did not start. Please try again.",
  );

  showSession(h);
  // A click can already be queued before the late snapshot updates the screen.
  fireEvent.click(retry);
  await act(async () => {});

  expect(h.store.get(kickoffSessionAtom)).toBe(session.id);
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  expect(window.confirm).not.toHaveBeenCalled();
  expect(h.send).toHaveBeenCalledOnce();
});

it.each([0, 5000])("releases the session subscription on unmount after %i ms", async (delay) => {
  vi.useFakeTimers();
  const h = mount();
  enterText();
  fireEvent.click(submitButton());
  h.result();
  await act(async () => {});
  await act(async () => {
    vi.advanceTimersByTime(delay);
  });

  h.unmount();
  showSession(h);

  expect(h.store.get(kickoffSessionAtom)).toBeNull();
});

it("keeps the draft in memory only", () => {
  const h = mount();
  enterText();
  h.unmount();

  mount();

  expect(composer().value).toBe("");
});
