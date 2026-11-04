import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { useState } from "react";
import { MemoryRouter, useLocation } from "react-router";
import { describe, expect, it } from "vitest";

import type { ProjectChecklistView } from "@/plan/api/schema";
import { snapshotAtom } from "@/plan/chat/model";
import { panelCollapsedAtom } from "@/plan/chat/prefs";
import { screenAtom } from "@/plan/chat/session-home";
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
      has_product: true,
      sections: [],
      plans: { listed: 0, total: 0, written: 0, running: 0, done: 0 },
      untested: 0,
      metaplans: [],
      plan_rows: [],
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
  "use git mv into doc/plans/search/, " +
  "keep file names, and fix every reference to it.";

function Where() {
  return <output data-testid="where">{useLocation().pathname}</output>;
}

function DialogHost({ paths }: { paths: string[] }) {
  const [open, setOpen] = useState(true);
  return (
    <MemoryRouter initialEntries={["/"]}>
      <MoveIntoFeatureDialog paths={paths} open={open} onOpenChange={setOpen} />
      <Where />
    </MemoryRouter>
  );
}

const where = () => screen.getByTestId("where").textContent;

function mount(path: string | string[] = source, current: typeof session | null = null) {
  const h = harness(current);
  h.store.set(projectAtom, project);
  const paths = typeof path === "string" ? [path] : path;
  return { ...h, ...h.mount(<DialogHost paths={paths} />) };
}

function confirmButton() {
  return screen.getByRole("button", { name: "Move" });
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
      feature: "search",
      text: expectedText,
      attachments: [],
      client_id: expect.any(String),
    });
    expect(screen.getByRole("dialog", { name: "Move into a feature" })).toBeTruthy();
    h.result();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    // `/` mounts no chat panel: the move's session shows on the destination feature's screen.
    await waitFor(() => expect(where()).toBe("/f/search"));
  });

  it("moves several plans in one message and lists them by title", async () => {
    const h = mount([source, "doc/plans/PLAN-cache.md"]);
    const dialog = screen.getByRole("dialog", { name: "Move into a feature" });
    expect(dialog.textContent).toContain("Moving 2 plans");
    expect(within(dialog).getByText("Fix search")).toBeTruthy();
    expect(within(dialog).getByText("doc/plans/PLAN-cache.md")).toBeTruthy();
    fireEvent.click(screen.getByRole("radio", { name: "search" }));

    fireEvent.click(confirmButton());

    expect(h.send).toHaveBeenCalledTimes(1);
    expect(h.send).toHaveBeenCalledWith(
      expect.objectContaining({
        feature: "search",
        text:
          'Move these plans into the feature "search" with git mv into doc/plans/search/, ' +
          "keeping file names and fixing every reference to them:\n" +
          "- doc/plans/PLAN-fix-search.md\n- doc/plans/PLAN-cache.md",
      }),
    );
    h.result();
    await waitFor(() => expect(where()).toBe("/f/search"));
  });

  it("refuses to confirm with nothing to move", () => {
    const h = mount([]);
    enterName("search");

    fireEvent.click(confirmButton());

    expect(confirmButton().getAttribute("aria-disabled")).toBe("true");
    expect(screen.getByText("Choose a standalone plan to move")).toBeTruthy();
    expect(h.send).not.toHaveBeenCalled();
  });

  it("opens the plan's page when the destination feature is new, and stays put on a refusal", async () => {
    const h = mount();
    enterName("new-feature");
    fireEvent.click(confirmButton());
    h.result(0, "Cannot move");
    await screen.findByRole("alert");
    expect(where()).toBe("/");

    fireEvent.click(confirmButton());
    h.result(1);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(where()).toBe("/plans/fix-search"));
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
    // The server refuses a start for a feature that does not exist yet.
    expect(h.send.mock.calls[0][0]).not.toHaveProperty("feature");
    h.result();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    // A misplaced document is no listed plan: its own page shows the chat, `/` has no panel.
    await waitFor(() => expect(where()).toBe("/files/product/index.md"));
  });

  it("explains why a read-only browser cannot confirm", () => {
    const h = harness();
    h.store.set(projectAtom, project);
    h.store.set(authoringAtom, {
      holder: { public_id: "other", name: "Alex", away_since: null, frees_at: null },
    });
    h.mount(<DialogHost paths={[source]} />);
    enterName("search");

    fireEvent.click(confirmButton());

    expect(screen.getAllByText("Alex holds authoring")).toHaveLength(1);
    expect(screen.queryByText("Authoring is unavailable")).toBeNull();
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

  it("refuses without asking while a project-wide chat is open, and starts once it ended", async () => {
    const current = { ...session, state: "idle" as const };
    const h = mount(source, current);
    enterName("search");

    fireEvent.click(confirmButton());

    expect(window.confirm).not.toHaveBeenCalled();
    expect(h.send).not.toHaveBeenCalled();
    expect((await screen.findByRole("alert")).textContent).toBe("Finish the open chat first.");
    expect(screen.getByRole("dialog")).toBeTruthy();
    act(() => {
      h.store.set(snapshotAtom, {
        session: { ...current, state: "ended" },
        turns: [],
        queue: [],
        previous: null,
      });
    });
    fireEvent.click(confirmButton());
    expect(h.send.mock.calls[0][0]).toMatchObject({ command: "start", text: expectedText });
    h.result();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("names the feature a running chat belongs to", async () => {
    const h = mount(source, { ...session, state: "idle", feature: "search" });
    act(() => h.store.set(screenAtom, { feature: null, plan: null }));
    enterName("other");

    fireEvent.click(confirmButton());

    expect(window.confirm).not.toHaveBeenCalled();
    expect(h.send).not.toHaveBeenCalled();
    expect((await screen.findByRole("alert")).textContent).toBe(
      "Finish the session in search first.",
    );
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

  it("a dialog closed during the start reveals no panel", async () => {
    const h = mount();
    h.store.set(panelCollapsedAtom, true);
    enterName("search");
    fireEvent.click(confirmButton());

    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    h.result();
    await act(async () => {});

    expect(h.store.get(panelCollapsedAtom)).toBe(true);
  });
});
