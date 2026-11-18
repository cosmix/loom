import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type StageStatus, type StageSummary } from "@/api/schema";
import { ThreadChips } from "@/components/stage-thread";
import { applySnapshot } from "@/state/apply";

const fixture = snapshotSchema.parse(fixtureJson);
const base = fixture.status.stages[0]!;

function make(id: string, dependencies: string[] = [], status: StageStatus = "completed") {
  return { ...base, id, name: `Name ${id}`, dependencies, status } satisfies StageSummary;
}

function renderThread(stages: StageSummary[], focus: string, pick = vi.fn()) {
  const store = createStore();
  applySnapshot(store, {
    ...structuredClone(fixture),
    status: { ...fixture.status, stages },
  });
  const stage = stages.find((entry) => entry.id === focus)!;
  render(
    <Provider store={store}>
      <ThreadChips stage={stage} pick={pick} />
    </Provider>,
  );
  return pick;
}

/// `count` upstream stages of `mid`, the first `blocked` ones in that state.
function upstream(count: number, blocked = 0) {
  const ups = Array.from({ length: count }, (_, i) =>
    make(`up-${i}`, [], i < blocked ? "blocked" : "completed"),
  );
  return [
    ...ups,
    make(
      "mid",
      ups.map((entry) => entry.id),
    ),
  ];
}

afterEach(cleanup);

describe("ThreadChips", () => {
  it("renders both runs around the current stage", () => {
    renderThread([make("a"), make("mid", ["a"]), make("z", ["mid"], "queued")], "mid");
    const waits = screen.getByRole("group", { name: "waits for" });
    const feeds = screen.getByRole("group", { name: "feeds" });
    expect(within(waits).getByRole("button", { name: /^a/ })).toBeTruthy();
    expect(within(feeds).getByRole("button", { name: /^z/ })).toBeTruthy();
    expect(within(feeds).getByTitle("Name z · Queued")).toBeTruthy();
    const self = screen.getByText("this stage").closest("[aria-current]");
    expect(self?.getAttribute("aria-current")).toBe("true");
    expect(self?.textContent).toBe("this stage");
    expect(self?.tagName).not.toBe("BUTTON");
  });

  it("caps an empty side", () => {
    renderThread([make("mid"), make("z", ["mid"])], "mid");
    expect(screen.getByTitle("No dependencies: this stage starts first")).toBeTruthy();
    expect(screen.queryByRole("group", { name: "waits for" })).toBeNull();
    cleanup();
    renderThread([make("a"), make("mid", ["a"])], "mid");
    expect(screen.getByTitle("Nothing waits on this stage")).toBeTruthy();
    expect(screen.queryByRole("group", { name: "feeds" })).toBeNull();
  });

  it("renders nothing for an isolated stage", () => {
    renderThread([make("mid")], "mid");
    expect(screen.queryByText("this stage")).toBeNull();
  });

  it("hands the clicked id to pick", () => {
    const pick = renderThread([make("a"), make("mid", ["a"])], "mid");
    fireEvent.click(screen.getByRole("button", { name: /^a/ }));
    expect(pick).toHaveBeenCalledWith("a");
  });

  it("folds a run of seven and expands it to seven chips", () => {
    renderThread(upstream(7, 1), "mid");
    const fold = screen.getByRole("button", {
      name: "waits for 7: 1 blocked, 6 completed",
    });
    expect(fold.getAttribute("aria-expanded")).toBe("false");
    expect(fold.querySelectorAll(".fulcrum-dot")).toHaveLength(7);
    expect(screen.queryByTitle("Name up-0 · Blocked")).toBeNull();
    fireEvent.click(fold);
    const waits = screen.getByRole("group", { name: "waits for" });
    expect(within(waits).getAllByTitle(/^Name up-/)).toHaveLength(7);
    const back = within(waits).getByRole("button", { name: "Fold waits for 7" });
    expect(back.getAttribute("aria-expanded")).toBe("true");
    fireEvent.click(back);
    expect(within(waits).getAllByRole("button")).toHaveLength(1);
  });

  it("does not fold a run of six", () => {
    renderThread(upstream(6), "mid");
    const waits = screen.getByRole("group", { name: "waits for" });
    expect(within(waits).getAllByRole("button")).toHaveLength(6);
    expect(waits.querySelector("[aria-expanded]")).toBeNull();
  });
});
