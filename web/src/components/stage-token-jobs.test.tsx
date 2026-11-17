import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { StageTokens } from "@/components/stage-tokens";
import { applySnapshot } from "@/state/apply";

afterEach(cleanup);

function renderTokens(stageId: string, mutate?: (raw: typeof fixtureJson) => void) {
  const raw = structuredClone(fixtureJson);
  mutate?.(raw);
  const store = createStore();
  applySnapshot(store, snapshotSchema.parse(raw));
  render(
    <Provider store={store}>
      <StageTokens stageId={stageId} />
    </Provider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "tokens" }));
}

describe("StageTokens codex jobs and effort", () => {
  it("titles the codex lane by its unit id and shows each lane's effort after its model", () => {
    renderTokens("server");

    const lanes = screen.getAllByTestId("token-lane");
    const codex = lanes[2] as HTMLElement;
    expect(within(codex).getByText("hex-test", { exact: true })).toBeTruthy();
    expect(within(codex).getByText("codex", { exact: true })).toBeTruthy();
    expect(codex.textContent).not.toContain("codex job");
    expect(codex.textContent).toContain("gpt-6.2-sol/xhigh · codex-01a1396b");
    expect(lanes[1]?.textContent).toContain("claude-sonnet-5-5/medium · a04baa183f888b773");
  });

  it("joins a codex lane to its job: outcome chip with its reason, run time and deadline", () => {
    renderTokens("server");

    const meta = within(screen.getAllByTestId("token-lane")[2] as HTMLElement).getByTestId(
      "codex-job-meta",
    );
    const chip = within(meta).getByText("succeeded");
    expect(chip.getAttribute("title")).toBe("turn completed");
    expect(chip.classList).toContain("tone-completed");
    expect(meta.textContent).toContain("23s / 25m");
  });

  it("shows a job that never got a thread as its own row in the codex group", () => {
    renderTokens("server");

    const group = screen.getByTestId("codex-native");
    const rows = within(group).getAllByTestId("codex-job-row");
    expect(rows).toHaveLength(1);
    const row = rows[0] as HTMLElement;
    expect(row.textContent).toContain("hex-lint");
    expect(row.textContent).toContain("gpt-6.2-sol/xhigh · no thread");
    const chip = within(row).getByText("failed");
    expect(chip.getAttribute("title")).toBe("codex exited before starting a thread");
    expect(chip.classList).toContain("tone-blocked");
    expect(row.textContent).toContain("2s / 25m");
    expect(row.textContent).toContain("—");
    expect(row.querySelector('[role="img"]')).toBeNull();
    expect(screen.getAllByTestId("token-lane")).toHaveLength(3);
  });

  it("puts every native codex lane and thread-less job under one codex heading", () => {
    renderTokens("server", (raw) => {
      const agents = raw.status.tokens.stages[0]?.agents;
      const job = agents?.[2];
      if (!agents || !job) throw new Error("fixture codex job is missing");
      const extra = { ...structuredClone(job), id: "codex-02b2407c", unit_id: "hex-extra" };
      agents.push(extra as unknown as typeof job);
    });

    expect(screen.getAllByText("codex", { selector: ".eyebrow" })).toHaveLength(1);
    const group = screen.getByTestId("codex-native");
    expect(within(group).getAllByTestId("token-lane")).toHaveLength(2);
    expect(within(group).getAllByTestId("codex-job-row")).toHaveLength(1);
    expect(screen.queryByTestId("codex-cluster")).toBeNull();
  });

  it("never shows a finished job's lane as live, however active its agent reads", () => {
    renderTokens("server", (raw) => {
      const job = raw.status.tokens.stages[0]?.agents[2];
      if (!job) throw new Error("fixture codex job is missing");
      job.active = true;
    });

    const dot = within(screen.getAllByTestId("token-lane")[2] as HTMLElement).getByTestId(
      "agent-dot",
    );
    expect(dot.dataset.state).toBe("idle");
    expect(dot.classList).not.toContain("dot-live");
  });

  it("shows no thread-less row for a stage without unthreaded jobs", () => {
    renderTokens("server", (raw) => {
      const job = raw.status.stages.find((stage) => stage.id === "server")?.codex_jobs;
      if (!job) throw new Error("fixture server stage is missing");
      job.splice(1, 1);
    });

    expect(screen.queryByTestId("codex-job-row")).toBeNull();
    expect(screen.getAllByTestId("token-lane")).toHaveLength(3);
  });
});
