import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { Attention, StageIssue } from "@/api/schema";
import { fixture, fixtureStage, renderModal } from "./stage-report-kit";

function fixtureEntry(id: string): Attention {
  const entry = fixture.attention.find((candidate) => candidate.id === id);
  if (!entry) throw new Error(`fixture attention entry ${id} is missing`);
  return entry;
}

function issue(overrides: Partial<StageIssue> = {}): StageIssue {
  return {
    kind: "failure",
    failure_type: "sandbox-setup-failure",
    detected_at: "2026-10-27T20:40:18Z",
    evidence: ["No adjudication session could be started for this dispute"],
    ended_at: "2026-10-27T20:41:32Z",
    ended_by: "replaced by a newer failure",
    disposition: "superseded",
    dispute_id: null,
    ...overrides,
  };
}

const region = (name: string) => within(screen.getByRole("region", { name }));
/// The history rows, opening the card first when it is folded.
const historyItems = () => {
  const toggle = region("history").getByRole("button", { name: "history" });
  if (toggle.getAttribute("aria-expanded") === "false") fireEvent.click(toggle);
  return region("history").getAllByRole("listitem");
};

/// `count` archived reviews an hour apart, newest first as the daemon sends them.
function reviews(count: number): StageIssue[] {
  return Array.from({ length: count }, (_, index) =>
    issue({
      kind: "review",
      failure_type: null,
      detected_at: null,
      evidence: [],
      ended_at: `2026-10-27T${String(19 - index).padStart(2, "0")}:00:00Z`,
      ended_by: "NeedsHumanReview -> Queued",
      disposition: "resolved",
    }),
  );
}

/// Makes every box report content taller than itself, as a long review does
/// in a browser; jsdom lays nothing out.
function overflowEverything() {
  vi.spyOn(Element.prototype, "scrollHeight", "get").mockReturnValue(1000);
  vi.spyOn(Element.prototype, "clientHeight", "get").mockReturnValue(288);
}

/// The stage's entry turned into one about something other than its failure.
function stashEntry(id: string): Attention {
  return {
    ...fixtureEntry(id),
    label: "STASH NOT RESTORED",
    status: "blocked",
    command: "git stash list",
    note: "restore them with `git stash pop`",
    failure_type: null,
    failure_label: null,
    evidence: [],
  };
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("the now card", () => {
  it("is absent without an attention entry and present with one", () => {
    renderModal(fixtureStage("server"), []);
    expect(screen.queryByRole("region", { name: "now" })).toBeNull();
    cleanup();

    renderModal(fixtureStage("docs"), [fixtureEntry("docs")]);
    expect(screen.getByRole("region", { name: "now" })).toBeTruthy();
  });

  it("shows only the current merge conflict, never the archived sandbox failure", () => {
    renderModal(fixtureStage("docs", { history: [issue()] }), [fixtureEntry("docs")]);

    const now = region("now");
    expect(now.getByText("Merge conflict; a merge resolver session fixes it.")).toBeTruthy();
    expect(now.getByText(/merge resolver session-3f9a2c1e-1788720000 is working/)).toBeTruthy();
    expect(now.getByText("what loom is doing")).toBeTruthy();
    expect(now.queryByText(/No adjudication session/)).toBeNull();
    expect(now.queryByText(/sandbox/i)).toBeNull();
  });

  it("shows the complete review notes in place of the cut reason", () => {
    overflowEverything();
    const notes = Array.from({ length: 12 }, (_, i) => `criterion ${i} was disputed`).join("\n");
    expect(notes.length).toBeGreaterThan(200);
    const entry = fixtureEntry("integration-verify");
    renderModal(fixtureStage("integration-verify", { review_notes: notes }), [entry]);

    const box = region("now").getByLabelText("review notes text");
    expect(box.textContent).toBe(notes);
    expect(box.className).toContain("whitespace-pre-wrap");
    expect(box.getAttribute("tabindex")).toBe("0");
    expect(region("now").queryByText(entry.reason ?? "")).toBeNull();
  });

  it.each(["queued", "executing"] as const)(
    "gives no failure time while a retry is %s",
    (status) => {
      const failure = fixtureStage("client").failure_info;
      renderModal(fixtureStage("server", { status, retry_count: 1, failure_info: failure }), [
        { ...stashEntry("client"), id: "server" },
      ]);

      expect(region("now").queryByText(/^since/)).toBeNull();
    },
  );

  it("says since when the current failure was detected", () => {
    renderModal(fixtureStage("client"), [fixtureEntry("client")]);

    expect(region("now").getByText(/^since/)).toBeTruthy();
  });

  it("makes a short text box plain text, neither a region nor a tab stop", () => {
    renderModal(fixtureStage("client"), [fixtureEntry("client")]);

    const box = region("now").getByText("cargo test failed").closest(".text-box");
    expect(box?.getAttribute("tabindex")).toBeNull();
    expect(box?.getAttribute("role")).toBeNull();
  });

  it("keeps the live failure's evidence and time under an entry about something else", () => {
    renderModal(fixtureStage("client"), [stashEntry("client")]);

    const now = region("now");
    expect(now.getByText("STASH NOT RESTORED")).toBeTruthy();
    expect(now.getByText("1 test failed: schema::parses_fixture")).toBeTruthy();
    expect(now.getByText(/^since/)).toBeTruthy();
  });

  it("dates no failure when the entry's stage has none live", () => {
    const stage = fixtureStage("client", { status: "completed" });
    renderModal(stage, [stashEntry("client")]);

    expect(region("now").queryByText("1 test failed: schema::parses_fixture")).toBeNull();
    expect(region("now").queryByText(/^since/)).toBeNull();
  });

  it("names a failed cleanup and shows its warning", () => {
    const warning = "git worktree remove failed: worktree contains modified files";
    const stage = fixtureStage("knowledge-bootstrap", { cleanup_warning: warning });
    const entry: Attention = {
      ...stashEntry("client"),
      id: stage.id,
      name: stage.name,
      label: "CLEANUP FAILED",
      command: `loom worktree remove ${stage.id}`,
      note: null,
      cleanup_warning: warning,
    };
    renderModal(stage, [entry]);

    const now = region("now");
    expect(now.getByText("CLEANUP FAILED")).toBeTruthy();
    expect(now.getByText(warning)).toBeTruthy();
    expect(now.getByText(`loom worktree remove ${stage.id}`)).toBeTruthy();
  });

  it("wears the entry's tone, as the attention card and the current history row do", () => {
    renderModal(fixtureStage("knowledge-bootstrap"), [
      { ...stashEntry("client"), id: "knowledge-bootstrap" },
    ]);

    expect(screen.getByRole("region", { name: "now" }).className).toContain("tone-blocked");
    expect(historyItems()[0]?.className).toContain("tone-blocked");
  });
});

describe("the history card", () => {
  it("puts the current issue on top and marks each archived one by how it ended", () => {
    const history = [
      issue(),
      issue({
        ended_by: "loom stage reset",
        disposition: "reset",
        ended_at: "2026-10-27T20:00:00Z",
      }),
    ];
    renderModal(fixtureStage("docs", { history }), [fixtureEntry("docs")]);

    const [current, superseded, reset] = historyItems();
    expect(current?.dataset.mark).toBe("current");
    expect(within(current!).getByText("Merge conflict")).toBeTruthy();
    expect(superseded?.dataset.mark).toBe("superseded");
    expect(within(superseded!).getByText("Sandbox setup failure")).toBeTruthy();
    expect(within(superseded!).getByText(/No adjudication session/)).toBeTruthy();
    expect(within(superseded!).getByText(/ended: replaced by a newer failure/)).toBeTruthy();
    expect(reset?.dataset.mark).toBe("reset");
  });

  it("lists newest first and folds to the six newest until shown all", () => {
    const history = reviews(9);
    renderModal(fixtureStage("docs", { history }), [fixtureEntry("docs")]);

    const shown = historyItems();
    expect(shown).toHaveLength(6);
    const times = shown.slice(1).map((item) => item.querySelector("time")?.dateTime);
    expect(times).toEqual(history.slice(0, 5).map((entry) => entry.ended_at));
    expect(region("history").getByText("10 events · newest first")).toBeTruthy();

    const toggle = screen.getByRole("button", { name: "Show all 10 events" });
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(toggle);

    expect(historyItems()).toHaveLength(10);
    const collapse = screen.getByRole("button", { name: "Collapse to recent" });
    expect(collapse.getAttribute("aria-expanded")).toBe("true");
    expect(region("history").getByText("1 merge conflict · 9 reviews")).toBeTruthy();
  });

  it("is folded to its header unless the history holds a failure", () => {
    renderModal(fixtureStage("docs", { history: reviews(2) }), [fixtureEntry("docs")]);

    const toggle = region("history").getByRole("button", { name: "history" });
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(region("history").queryAllByRole("listitem")).toHaveLength(0);
    expect(region("history").getByText("3 events · newest first")).toBeTruthy();

    fireEvent.click(toggle);
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    expect(region("history").getAllByRole("listitem")).toHaveLength(3);
    fireEvent.click(toggle);
    expect(region("history").queryAllByRole("listitem")).toHaveLength(0);
  });

  it("opens by itself on an archived failure and folds on request", () => {
    renderModal(fixtureStage("docs", { history: [issue()] }), [fixtureEntry("docs")]);

    const toggle = region("history").getByRole("button", { name: "history" });
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    expect(region("history").getAllByRole("listitem")).toHaveLength(2);
    fireEvent.click(toggle);
    expect(region("history").queryAllByRole("listitem")).toHaveLength(0);
  });

  it("has no show-all control while every event fits", () => {
    renderModal(fixtureStage("docs", { history: reviews(5) }), [fixtureEntry("docs")]);

    expect(historyItems()).toHaveLength(6);
    expect(screen.queryByRole("button", { name: /^Show all/ })).toBeNull();
  });

  it("scrolls into view when the dialog opens at #history", () => {
    const scroll = vi.fn();
    Object.defineProperty(Element.prototype, "scrollIntoView", {
      configurable: true,
      value: scroll,
    });
    try {
      renderModal(fixtureStage("docs", { history: [issue()] }), [fixtureEntry("docs")], "#history");

      expect(scroll).toHaveBeenCalledWith({ block: "start" });
      expect(scroll.mock.contexts[0]).toBe(screen.getByRole("region", { name: "history" }));
    } finally {
      delete (Element.prototype as Partial<Element>).scrollIntoView;
    }
  });

  it("shows the day beside the clock for an event before today", () => {
    renderModal(fixtureStage("docs", { history: [issue({ ended_at: "2020-01-02T03:04:05Z" })] }), [
      fixtureEntry("docs"),
    ]);

    const time = historyItems()[1]?.querySelector("time");
    expect(time?.querySelector(".moment-day")?.textContent).toMatch(/2020/);
    expect(time?.getAttribute("title")).toMatch(/2020/);
  });

  it("is absent for a stage with no issue at all", () => {
    renderModal(fixtureStage("server"), []);

    expect(screen.queryByRole("region", { name: "history" })).toBeNull();
  });
});

describe("long text", () => {
  it("renders a 400-character evidence line whole in now and in history", () => {
    const line = `${"x".repeat(420)} end`;
    const entry = { ...fixtureEntry("client"), evidence: [line] };
    renderModal(fixtureStage("client", { history: [issue({ evidence: [line] })] }), [entry]);

    expect(region("now").getByText(line).textContent).toHaveLength(line.length);
    const archived = historyItems()[1]!;
    expect(within(archived).getByText(line).closest(".text-box")).toBeTruthy();
  });

  it.each(["docs", "server", "local-runtime"])(
    "carries no truncating class in the %s dialog, history and details open",
    (id) => {
      const entry = fixture.attention.filter((candidate) => candidate.id === id);
      renderModal(fixtureStage(id, { history: reviews(9) }), entry);
      historyItems();
      fireEvent.click(screen.getByRole("button", { name: /^Show all/ }));
      fireEvent.click(screen.getByRole("button", { name: /^details/ }));

      const dialog = screen.getByRole("dialog");
      const cut = [...dialog.querySelectorAll("[class]")].filter((element) =>
        /\b(truncate|line-clamp-\S+|text-ellipsis)\b/.test(element.getAttribute("class") ?? ""),
      );
      expect(cut).toEqual([]);
    },
  );
});
