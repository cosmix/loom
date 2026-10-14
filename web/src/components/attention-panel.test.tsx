import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type Attention } from "@/api/schema";
import {
  AttentionBody,
  AttentionPanel,
  attentionHazard,
  attentionStatus,
} from "@/components/attention-panel";
import { applySnapshot } from "@/state/apply";

const fixture = snapshotSchema.parse(fixtureJson);

function fixtureEntry(id: string): Attention {
  const entry = fixture.attention.find((candidate) => candidate.id === id);
  if (!entry) throw new Error(`fixture attention entry ${id} is missing`);
  return entry;
}

function renderPanel(attention: Attention[]) {
  const store = createStore();
  applySnapshot(store, { ...structuredClone(fixture), attention });
  const router = createMemoryRouter([{ path: "/", element: <AttentionPanel /> }]);
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
}

afterEach(cleanup);

describe("AttentionBody", () => {
  it("renders exactly the three review decisions for NEEDS REVIEW, each in full", () => {
    render(<AttentionBody entry={fixtureEntry("integration-verify")} />);

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
        entry={{ ...fixtureEntry("client"), label: "NEEDS INPUT", command: null, note }}
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
    render(<AttentionBody entry={entry} />);

    expect(screen.getByText("retry limit reached (3/3)").tagName).toBe("P");
    expect(screen.getByText("loom stage retry client --force").tagName).toBe("CODE");
    expect(
      screen.getAllByRole("button").map((button) => button.getAttribute("aria-label")),
    ).toEqual(["copy loom stage retry client --force"]);
  });
});

describe("attentionHazard", () => {
  it("never paints an automatic entry as an error", () => {
    const blocked = { ...fixtureEntry("client"), label: "BLOCKED" };

    expect(attentionHazard(blocked)).toBe("error");
    expect(attentionHazard({ ...blocked, automatic: true })).toBe("warning");
  });
});

describe("attention labels", () => {
  it("lists the typed merge block under handled by loom in the merge-blocked state", () => {
    const entry = fixtureEntry("merge-gate");
    expect(entry.label).toBe("MERGE BLOCKED");
    expect(attentionStatus(entry)).toBe("merge-blocked");
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
      command: "git stash list",
      note: "restore them with `git stash pop`",
      automatic: false,
      failure_type: null,
      failure_label: null,
      evidence: [],
    };
    expect(attentionStatus(entry)).toBe("blocked");
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
    render(<AttentionBody entry={entry()} />);

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

  it("introduces the retry as the step after the reason and lists the other ways out", () => {
    render(<AttentionBody entry={entry()} />);

    expect(screen.getByText(/^Once you have done what the reason asks/)).toBeTruthy();
    const options = within(screen.getByRole("list", { name: "other options" }));
    expect(options.getAllByRole("listitem")).toHaveLength(3);
    expect(options.getByText(/^If the reason is an acceptance criterion/).textContent).toContain(
      "find the criterion's index in the acceptance list",
    );
    expect(options.getByText(/^If you give up on the stage/)).toBeTruthy();
  });

  it("gives the retry and each alternative its own copy button", () => {
    render(<AttentionBody entry={entry()} />);

    expect(
      screen.getAllByRole("button").map((button) => button.getAttribute("aria-label")),
    ).toEqual([
      'copy loom stage retry local-runtime --context "<what you did and what it showed>"',
      "copy loom stage amend local-runtime --field acceptance --op delete --index <n>",
      'copy loom stage amend local-runtime --field acceptance --op replace --index <n> --value "<new command>"',
      "copy loom stage skip local-runtime",
    ]);
  });

  it("leaves out the option list and the introduction when an entry has none", () => {
    render(<AttentionBody entry={fixtureEntry("client")} />);

    expect(screen.queryByRole("list", { name: "other options" })).toBeNull();
    expect(screen.queryByText(/^Once /)).toBeNull();
  });
});
