import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";

import type { ProjectChecklistView } from "@/plan/api/schema";
import { snapshotAtom } from "@/plan/chat/model";
import { harness, installStartChatLifecycle, session } from "@/plan/chat/start-chat-test-kit";
import { MoveIntoFeatureDialog } from "@/plan/home/move-into-feature";
import { FEATURE_NAME_HINT } from "@/plan/new/feature-name";
import { authoringAtom, projectAtom } from "@/plan/state/atoms";

installStartChatLifecycle();

const project: ProjectChecklistView = {
  checklists: [
    {
      feature: "search",
      pitch: null,
      title: "Search",
      has_product: true,
      sections: [],
      plans: { listed: 0, total: 0, written: 0, running: 0, done: 0 },
      untested: 0,
      current: { kind: "ready" },
      steps: [],
      primary: null,
      secondary: [],
      items: [],
    },
  ],
  standalone: [
    {
      slug: "fix-search",
      path: "doc/plans/PLAN-fix-search.md",
      title: "Fix search",
      state: "written",
    },
  ],
  moves: [],
};
const source = "doc/plans/PLAN-fix-search.md";
const expectedText =
  'Move doc/plans/PLAN-fix-search.md into the feature "search": ' +
  "use git mv into the matching doc/product/search/ or doc/plans/search/ directory, " +
  "keep file names, and fix every reference to it.";

function DialogHost({ path }: { path: string | null }) {
  const [open, setOpen] = useState(true);
  return <MoveIntoFeatureDialog path={path} open={open} onOpenChange={setOpen} />;
}

function mount(path: string | null = source, current: typeof session | null = null) {
  const h = harness(current);
  h.store.set(projectAtom, project);
  return { ...h, ...h.mount(<DialogHost path={path} />) };
}

function confirmButton() {
  return screen.getByRole("button", { name: "Move into feature" });
}

function enterName(name: string) {
  fireEvent.change(screen.getByRole("textbox", { name: "New feature name" }), {
    target: { value: name },
  });
}

describe("MoveIntoFeatureDialog", () => {
  it("describes the new feature input with the shared naming hint", () => {
    mount();

    const input = screen.getByRole("textbox", { name: "New feature name" });
    const hint = screen.getByText(FEATURE_NAME_HINT);
    expect(input.getAttribute("aria-describedby")).toBe(hint.id);
  });

  it.each([
    "",
    "Archive",
    "-search",
    "with space",
    "with/slash",
    "café",
    "a_b",
    "archive",
    "briefs",
    "plans",
    "overview",
  ])("rejects invalid feature name %j", (name) => {
    const h = mount();

    enterName(name);
    fireEvent.click(confirmButton());

    expect(confirmButton().getAttribute("aria-disabled")).toBe("true");
    expect(h.send).not.toHaveBeenCalled();
  });

  it.each(["a", "0", "new-feature", "1-feature", "feature-"])(
    "accepts valid feature name %j",
    (name) => {
      mount();

      enterName(name);

      expect(confirmButton().getAttribute("aria-disabled")).toBeNull();
    },
  );

  it("starts the exact move conversation for an existing feature and closes after acceptance", async () => {
    const h = mount();
    fireEvent.click(screen.getByRole("radio", { name: "search" }));

    fireEvent.click(confirmButton());

    expect(h.send).toHaveBeenCalledWith({
      type: "chat",
      command: "start",
      offer: null,
      text: expectedText,
      attachments: [],
      client_id: expect.any(String),
    });
    expect(screen.getByRole("dialog", { name: "Move into a feature" })).toBeTruthy();
    h.result();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("selects a standalone plan when no path is supplied", async () => {
    const h = mount(null);
    enterName("search");
    expect(confirmButton().getAttribute("aria-disabled")).toBe("true");

    fireEvent.click(screen.getByRole("radio", { name: "Fix search doc/plans/PLAN-fix-search.md" }));
    fireEvent.click(confirmButton());

    expect(h.send).toHaveBeenCalledWith(expect.objectContaining({ text: expectedText }));
    h.result();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("starts a move for a newly named feature and normalizes a doc-relative source", async () => {
    const h = mount("product/index.md");
    enterName("new-feature");

    fireEvent.click(confirmButton());

    expect(h.send).toHaveBeenCalledWith(
      expect.objectContaining({
        text:
          'Move doc/product/index.md into the feature "new-feature": ' +
          "use git mv into the matching doc/product/new-feature/ or doc/plans/new-feature/ " +
          "directory, keep file names, and fix every reference to it.",
      }),
    );
    h.result();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("explains why a read-only browser cannot confirm", () => {
    const h = harness();
    h.store.set(projectAtom, project);
    h.store.set(authoringAtom, {
      holder: { public_id: "other", name: "Alex", away_since: null, frees_at: null },
    });
    h.mount(<DialogHost path={source} />);
    enterName("search");

    fireEvent.click(confirmButton());

    expect(screen.getByText("Alex holds authoring")).toBeTruthy();
    expect(confirmButton().getAttribute("aria-disabled")).toBe("true");
    expect(h.send).not.toHaveBeenCalled();
  });

  it("shows a start refusal through Untrusted and leaves the dialog open", async () => {
    const h = mount();
    enterName("search");
    fireEvent.click(confirmButton());

    h.result(0, "Cannot\u202Emove");

    expect((await screen.findByRole("alert")).textContent).toBe("Cannot⟨U+202E⟩move");
    expect(confirmButton().getAttribute("aria-disabled")).toBeNull();
  });

  it("waits for the running chat to end before starting the move", async () => {
    const current = { ...session, state: "idle" as const };
    const h = mount(source, current);
    enterName("search");

    fireEvent.click(confirmButton());

    expect(window.confirm).toHaveBeenCalledWith(
      "End the current chat first? It will write its open questions into the documents before it stops.",
    );
    expect(h.send.mock.calls[0][0]).toMatchObject({ command: "end" });
    h.result();
    await act(async () => {});
    expect(h.send).toHaveBeenCalledTimes(1);
    act(() => {
      h.store.set(snapshotAtom, {
        session: { ...current, state: "ended" },
        turns: [],
        queue: [],
        previous: null,
      });
    });
    await waitFor(() => expect(h.send).toHaveBeenCalledTimes(2));
    expect(h.send.mock.calls[1][0]).toMatchObject({ command: "start", text: expectedText });
    h.result(1);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("preserves the current chat when ending it is cancelled", () => {
    const h = mount(source, { ...session, state: "idle" });
    vi.mocked(window.confirm).mockReturnValue(false);
    enterName("search");

    fireEvent.click(confirmButton());

    expect(h.send).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("rechecks authoring after the old chat ends", async () => {
    const current = { ...session, state: "idle" as const };
    const h = mount(source, current);
    enterName("search");
    fireEvent.click(confirmButton());
    h.result();
    await act(async () => {});

    act(() => {
      h.store.set(authoringAtom, {
        holder: { public_id: "other", name: "Alex", away_since: null, frees_at: null },
      });
      h.store.set(snapshotAtom, {
        session: { ...current, state: "ended" },
        turns: [],
        queue: [],
        previous: null,
      });
    });

    expect((await screen.findByRole("alert")).textContent).toBe("Alex holds authoring");
    expect(h.send).toHaveBeenCalledTimes(1);
  });

  it("blocks duplicate starts while waiting for the server", async () => {
    const h = mount();
    enterName("search");

    fireEvent.click(confirmButton());
    fireEvent.click(confirmButton());

    expect(h.send).toHaveBeenCalledTimes(1);
    expect(screen.getByText("Starting the move conversation")).toBeTruthy();
    h.result();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("cancels an end wait when the dialog is closed", async () => {
    const current = { ...session, state: "idle" as const };
    const h = mount(source, current);
    enterName("search");
    fireEvent.click(confirmButton());
    h.result();
    await act(async () => {});

    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    act(() => {
      h.store.set(snapshotAtom, {
        session: { ...current, state: "ended" },
        turns: [],
        queue: [],
        previous: null,
      });
    });
    await act(async () => {});

    expect(h.send).toHaveBeenCalledTimes(1);
  });
});
