import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type Attention, type Snapshot, type StageIssue } from "@/api/schema";
import { AttentionBody, AttentionPanel } from "@/components/attention-panel";
import { attentionDetail, attentionHazard, attentionStatus } from "@/components/attention-status";
import { applySnapshot } from "@/state/apply";
import { completionBlocker } from "@/test/completion-blocker";

const fixture = snapshotSchema.parse(fixtureJson);

function fixtureEntry(id: string): Attention {
  const entry = fixture.attention.find((candidate) => candidate.id === id);
  if (!entry) throw new Error(`fixture attention entry ${id} is missing`);
  return entry;
}

function renderPanel(attention: Attention[], edit: (snapshot: Snapshot) => void = () => {}) {
  const store = createStore();
  const snapshot = { ...structuredClone(fixture), attention };
  edit(snapshot);
  applySnapshot(store, snapshot);
  const router = createMemoryRouter([{ path: "/", element: <AttentionPanel /> }]);
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
}

function renderBody(entry: Attention) {
  const router = createMemoryRouter([{ path: "/", element: <AttentionBody entry={entry} /> }]);
  render(<RouterProvider router={router} />);
}

afterEach(cleanup);

describe("AttentionBody", () => {
  it("renders exactly the three review decisions for NEEDS REVIEW, each in full", () => {
    renderBody(fixtureEntry("integration-verify"));

    const commands = screen
      .getAllByText(/^loom stage human-review /)
      .map((element) => element.textContent);
    expect(commands).toEqual([
      "loom stage human-review integration-verify --approve",
      "loom stage human-review integration-verify --force-complete",
      'loom stage human-review integration-verify --reject "<reason>"',
    ]);
    expect(screen.getAllByRole("button")).toHaveLength(3);
  });

  it("renders a note as prose with no copy button", () => {
    const note = "the agent is waiting on a question: answer it in the stage's terminal";
    render(
      <AttentionBody
        entry={{
          ...fixtureEntry("client"),
          label: "NEEDS INPUT",
          status: "waiting-for-input",
          command: null,
          note,
        }}
      />,
    );

    expect(screen.getByText(note).tagName).toBe("P");
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("gives the command, and only the command, a copy button", () => {
    const entry = {
      ...fixtureEntry("client"),
      command: "loom stage retry client --force",
      note: "retry limit reached (3/3)",
    };
    renderBody(entry);

    expect(screen.getByText("retry limit reached (3/3)").tagName).toBe("P");
    expect(screen.getByText("loom stage retry client --force").tagName).toBe("CODE");
    expect(
      screen.getAllByRole("button").map((button) => button.getAttribute("aria-label")),
    ).toEqual(["copy loom stage retry client --force"]);
  });

  it("shows a cleanup warning on an entry with no failure or review reason", () => {
    const warning = "git worktree remove failed: worktree contains modified files";
    renderBody({
      ...fixtureEntry("client"),
      label: "CLEANUP FAILED",
      status: "blocked",
      command: "loom worktree remove client",
      failure_type: null,
      failure_label: null,
      evidence: [],
      review_reason: null,
      cleanup_warning: warning,
    });

    expect(screen.getByText(warning).tagName).toBe("P");
    expect(screen.getByText("loom worktree remove client").tagName).toBe("CODE");
  });
});

describe("attentionHazard", () => {
  it("never paints an automatic entry as an error", () => {
    const blocked = { ...fixtureEntry("client"), label: "BLOCKED", status: "blocked" as const };

    expect(attentionHazard(blocked)).toBe("error");
    expect(attentionHazard({ ...blocked, automatic: true })).toBe("warning");
  });
});

describe("completion pending card", () => {
  it("wears the stage's completion state, not the entry's blocked filing", () => {
    const entry: Attention = {
      ...fixtureEntry("client"),
      id: "server",
      name: "Rust server",
      label: "COMPLETION PENDING",
      status: "blocked",
      automatic: true,
    };
    renderPanel([entry], (snapshot) => {
      const server = snapshot.status.stages.find((stage) => stage.id === "server")!;
      server.completion_blocker = completionBlocker();
    });

    const card = screen.getByText("COMPLETION PENDING").closest(".bg-card");
    expect(card?.classList.contains("tone-warning")).toBe(true);
    expect(card?.classList.contains("tone-blocked")).toBe(false);
  });
});

describe("attention labels", () => {
  it("lists the typed merge block under handled by loom in the merge-blocked state", () => {
    const entry = fixtureEntry("merge-gate");
    expect(entry.label).toBe("MERGE BLOCKED");
    expect(attentionStatus(entry, "merge-blocked")).toBe("merge-blocked");
    expect(attentionHazard(entry)).toBe("warning");

    renderPanel([entry]);
    const automatic = screen.getByRole("region", { name: "handled by loom" });
    expect(within(automatic).getByText("MERGE BLOCKED")).toBeTruthy();
    expect(within(automatic).getByText(/loom retries the merge automatically/)).toBeTruthy();
  });

  it("lists unrestored stashed changes under needs attention as an error", () => {
    const entry: Attention = {
      ...fixtureEntry("client"),
      label: "STASH NOT RESTORED",
      status: "blocked",
      command: "git stash list",
      note: "restore them with `git stash pop`",
      automatic: false,
      failure_type: null,
      failure_label: null,
      evidence: [],
    };
    expect(attentionStatus(entry, null)).toBe("blocked");
    expect(attentionHazard(entry)).toBe("error");

    renderPanel([entry]);
    const operator = screen.getByRole("region", { name: "needs attention" });
    expect(within(operator).getByText("STASH NOT RESTORED")).toBeTruthy();
    expect(within(operator).getByText("git stash list").tagName).toBe("CODE");
  });
});

describe("AttentionPanel", () => {
  it("lists automatic entries under handled by loom and the rest under needs attention", () => {
    renderPanel(fixture.attention);

    const operator = screen.getByRole("region", { name: "needs attention" });
    const automatic = screen.getByRole("region", { name: "handled by loom" });
    expect(within(operator).getByText("ACCEPTANCE FAILED")).toBeTruthy();
    expect(within(operator).getByText("NEEDS REVIEW")).toBeTruthy();
    expect(within(operator).queryByText("MERGE CONFLICT")).toBeNull();
    expect(within(automatic).getByText("MERGE CONFLICT")).toBeTruthy();
    expect(within(automatic).queryByText("ACCEPTANCE FAILED")).toBeNull();
  });

  it("hides the handled by loom group when no entry is automatic", () => {
    renderPanel(fixture.attention.filter((entry) => !entry.automatic));

    expect(screen.getByRole("region", { name: "needs attention" })).toBeTruthy();
    expect(screen.queryByRole("region", { name: "handled by loom" })).toBeNull();
  });

  it("hides the needs attention group when every entry is automatic", () => {
    renderPanel(fixture.attention.filter((entry) => entry.automatic));

    expect(screen.getByRole("region", { name: "handled by loom" })).toBeTruthy();
    expect(screen.queryByRole("region", { name: "needs attention" })).toBeNull();
  });
});

describe("a deliberate block", () => {
  const entry = () => fixtureEntry("local-runtime");
  const reason = () => {
    const note = entry().note;
    if (!note) throw new Error("the fixture block has no note");
    return note;
  };

  it("shows the whole reason, command included, wrapped and never clamped", () => {
    renderBody(entry());

    const note = screen.getByText(
      (_, element) => element?.tagName === "P" && element.textContent === reason(),
    );
    expect(note.textContent).toContain(
      "From the worktree root run: TROOFOO_STACK=ephemeral bun run stack:ephemeral --with objectstore",
    );
    expect(note.textContent).toContain("Or allow AF_UNIX for this stage.");
    expect(note.className).toContain("whitespace-pre-wrap");
    expect(note.className).toContain("break-words");
    expect(note.className).not.toMatch(/truncate|line-clamp|text-ellipsis/);
  });

  it("numbers the retry first and the other ways out after it", () => {
    renderBody(entry());

    const steps = within(screen.getByRole("list", { name: "steps" }));
    const items = steps.getAllByRole("listitem");
    expect(items).toHaveLength(4);
    expect(items.map((item) => item.querySelector(".index-badge")?.textContent)).toEqual([
      "1",
      "2",
      "3",
      "4",
    ]);
    expect(within(items[0]!).getByText(/^Once you have done what the reason asks/)).toBeTruthy();
    expect(within(items[1]!).getByText(/^If criterion 4 cannot pass/)).toBeTruthy();
    expect(within(items[3]!).getByText(/^If you give up on the stage/)).toBeTruthy();
  });

  it("gives the retry and each alternative its own copy button", () => {
    renderBody(entry());

    expect(
      screen.getAllByRole("button").map((button) => button.getAttribute("aria-label")),
    ).toEqual([
      'copy loom stage retry local-runtime --context "<what you did and what it showed>"',
      "copy loom stage amend local-runtime --field acceptance --op delete --index 4",
      'copy loom stage amend local-runtime --field acceptance --op replace --index 4 --value "<new command>"',
      "copy loom stage skip local-runtime",
    ]);
  });

  it("lists the failing criterion by its index, command and diagnostic", () => {
    renderBody(entry());

    const failing = within(screen.getByRole("list", { name: "failing criteria" }));
    expect(failing.getByText("#4")).toBeTruthy();
    expect(failing.getByText(/stack:ephemeral --with objectstore/).tagName).toBe("CODE");
    expect(failing.getByText(/cannot create AF_UNIX sockets/)).toBeTruthy();
  });

  it("links the terminal history instead of printing a shell command", () => {
    renderBody(entry());

    const link = screen.getByRole("link", { name: "View terminal history" });
    expect(link.getAttribute("href")).toBe("/terminal/local-runtime");
    expect(document.body.textContent).not.toContain("less -R");
  });

  it("omits the block facts for an entry that has none", () => {
    renderBody(fixtureEntry("client"));

    expect(screen.queryByRole("list", { name: "failing criteria" })).toBeNull();
    expect(screen.queryByRole("link", { name: "View terminal history" })).toBeNull();
  });

  it("numbers a lone command as the only step, with no introduction", () => {
    renderBody(fixtureEntry("client"));

    const steps = within(screen.getByRole("list", { name: "steps" }));
    expect(steps.getAllByRole("listitem")).toHaveLength(1);
    expect(screen.queryByText(/^Once /)).toBeNull();
    expect(screen.queryByText(/^If /)).toBeNull();
  });
});

describe("attentionDetail", () => {
  it("never repeats the reason the body shows", () => {
    expect(fixtureEntry("local-runtime").reason).not.toBeNull();
    expect(attentionDetail(fixtureEntry("local-runtime"))).toBeNull();
  });

  it("composes the failure label and the adjudication numbers", () => {
    expect(attentionDetail(fixtureEntry("client"))).toBe("test");
    const adjudicating = {
      ...fixtureEntry("client"),
      dispute_count: 2,
      judge_heartbeat_secs: 42,
    };
    expect(attentionDetail(adjudicating)).toBe("test · 2 disputed · judge heard 42s ago");
  });
});

describe("the attention card", () => {
  it("uses the typed status when the human label changes", () => {
    const entry = { ...fixtureEntry("docs"), label: "new display text" };
    expect(attentionStatus(entry, "merge-conflict")).toBe("merge-conflict");
  });
  const archived: StageIssue = {
    kind: "failure",
    failure_type: "sandbox-setup-failure",
    detected_at: "2026-10-27T20:40:18Z",
    evidence: ["No adjudication session could be started for this dispute"],
    ended_at: "2026-10-27T20:41:32Z",
    ended_by: "replaced by a newer failure",
    disposition: "superseded",
    dispute_id: null,
  };
  const withHistory = (snapshot: Snapshot) => {
    const docs = snapshot.status.stages.find((stage) => stage.id === "docs");
    if (!docs) throw new Error("fixture stage docs is missing");
    docs.history = [archived, { ...archived, ended_at: "2026-10-27T20:30:00Z" }];
  };

  it("shows the current issue alone and links its earlier ones into the dialog history", () => {
    renderPanel([fixtureEntry("docs")], withHistory);

    const link = screen.getByRole("link", { name: "2 earlier issues" });
    expect(link.getAttribute("href")).toBe("/?stage=docs#history");
    expect(screen.queryByText(/No adjudication session could be started/)).toBeNull();
    expect(screen.getByText(/merge resolver session-3f9a2c1e-1788720000 is working/)).toBeTruthy();
  });

  it("has no earlier issues link for a stage without history", () => {
    renderPanel([fixtureEntry("docs")]);

    expect(screen.queryByRole("link", { name: /earlier issue/ })).toBeNull();
  });

  it("wraps the header detail instead of cutting it", () => {
    renderPanel([fixtureEntry("client")]);

    const detail = document.querySelector(".attention-detail");
    expect(detail?.textContent).toBe("test");
    expect(detail?.className).not.toMatch(/truncate|line-clamp|text-ellipsis|max-w-/);
    expect(detail?.getAttribute("title")).toBeNull();
    const slot = detail?.parentElement?.className.split(" ");
    expect(slot).toContain("min-w-0");
    expect(slot).toContain("shrink");
    expect(slot).not.toContain("shrink-0");
  });

  it("shows a block's reason once, in the body and never in the header", () => {
    const entry = fixtureEntry("local-runtime");
    renderPanel([entry]);

    expect(document.querySelector(".attention-detail")).toBeNull();
    const firstLine = entry.note?.split("\n", 1)[0] ?? "";
    expect(firstLine.length).toBeGreaterThan(0);
    expect(screen.getAllByText((text) => text.includes(firstLine))).toHaveLength(1);
  });
});
