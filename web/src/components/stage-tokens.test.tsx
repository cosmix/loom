import { act, cleanup, render, screen, within } from "@testing-library/react";
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
  return {
    raw,
    store,
    ...render(
      <Provider store={store}>
        <StageTokens stageId={stageId} />
      </Provider>,
    ),
  };
}

describe("StageTokens", () => {
  it("renders one lane per agent with a codex marker on the codex job", () => {
    renderTokens("server");

    const lanes = screen.getAllByTestId("token-lane");
    expect(lanes).toHaveLength(3);
    expect(lanes.filter((lane) => lane.textContent?.includes("codex"))).toHaveLength(1);
    expect(lanes[0]?.textContent).toContain("main session");
    expect(lanes[0]?.textContent).toContain("claude-opus-5-5 · main-01ed9f4c");
    expect(lanes[0]?.textContent).not.toMatch(/\b(live|idle)\b/);
    const dot = within(lanes[0] as HTMLElement).getByRole("img", { name: "live" });
    expect(dot.dataset.state).toBe("live");
    expect(dot.classList).toContain("dot-live");
  });

  it("keeps the model before the agent id, which wraps instead of truncating", () => {
    renderTokens("server");

    const lane = within(screen.getAllByTestId("token-lane")[0] as HTMLElement);
    const model = lane.getByText("claude-opus-5-5", { exact: true });
    const id = lane.getByText("main-01ed9f4c", { exact: true });
    expect(model.compareDocumentPosition(id) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(model.classList).toContain("shrink-0");
    expect(model.classList).not.toContain("truncate");
    expect(model.parentElement).toBe(id.parentElement);
    for (const className of ["flex", "min-w-0", "items-baseline"]) {
      expect(model.parentElement?.classList).toContain(className);
    }
    for (const className of ["wrap-anywhere", "min-w-0"]) {
      expect(id.classList).toContain(className);
    }
    expect(id.classList).not.toContain("truncate");
  });

  it("gives the agent id its full value as a hover title", () => {
    const fullId = "main-01ed9f4c-very-long-session-identifier";
    renderTokens("server", (raw) => {
      const agent = raw.status.tokens.stages[0]?.agents[0];
      if (!agent) throw new Error("fixture main agent is missing");
      agent.id = fullId;
    });

    const lane = within(screen.getAllByTestId("token-lane")[0] as HTMLElement);
    expect(lane.getByText(fullId, { exact: true }).getAttribute("title")).toBe(fullId);
  });

  it("clusters the codex job with its forwarder subagents after the other lanes", () => {
    renderTokens("server", (raw) => {
      const agents = raw.status.tokens.stages[0]?.agents;
      const forwarder = agents?.[1];
      if (!agents || !forwarder) throw new Error("fixture agents are missing");
      agents.push({
        ...structuredClone(forwarder),
        id: "a0forwarder",
        agent_type: "loom-codex-forwarder",
      });
      const job = agents?.[2];
      if (!job) throw new Error("fixture codex job is missing");
      (job as { forwarder: string | null }).forwarder = "a0forwarder";
    });

    const cluster = screen.getByTestId("codex-cluster");
    const inCluster = within(cluster).getAllByTestId("token-lane");
    expect(inCluster).toHaveLength(2);
    expect(inCluster[0]?.textContent).toContain("loom-codex-forwarder");
    expect(inCluster[0]?.textContent).toContain("claude-sonnet-5-5 · a0forwarder");
    expect(inCluster[1]?.textContent).toContain("codex job");
    const all = screen.getAllByTestId("token-lane");
    expect(all).toHaveLength(4);
    expect(all.slice(0, 2).every((lane) => !cluster.contains(lane))).toBe(true);
  });

  it("gives each forwarder and its codex job a cluster of their own", () => {
    renderTokens("server", (raw) => {
      const agents = raw.status.tokens.stages[0]?.agents;
      const forwarder = agents?.[1];
      const job = agents?.[2];
      if (!agents || !forwarder || !job) throw new Error("fixture agents are missing");
      for (const n of ["1", "2"]) {
        agents.push({
          ...structuredClone(forwarder),
          id: `a0fwd${n}`,
          agent_type: "loom-codex-forwarder",
        });
        agents.push({
          ...structuredClone(job),
          id: `codex-job${n}`,
          forwarder: `a0fwd${n}` as unknown as null,
        });
      }
      job.forwarder = null;
    });

    const clusters = screen.getAllByTestId("codex-cluster");
    const texts = clusters.map((c) =>
      within(c)
        .getAllByTestId("token-lane")
        .map((lane) => lane.textContent ?? ""),
    );
    expect(texts).toHaveLength(3);
    expect(texts[0]).toHaveLength(1);
    expect(texts[0]?.[0]).toContain("codex-01a1396b");
    expect(texts[1]).toHaveLength(2);
    expect(texts[1]?.[0]).toContain("a0fwd1");
    expect(texts[1]?.[1]).toContain("codex-job1");
    expect(texts[2]?.[0]).toContain("a0fwd2");
    expect(texts[2]?.[1]).toContain("codex-job2");
  });

  it("keeps every strip at 60 cells, newest on the right, however long the series", () => {
    const cellCounts = (stageSeries: number[]) => {
      renderTokens("server", (raw) => {
        const stage = raw.status.tokens.stages[0];
        if (!stage) throw new Error("fixture server stage is missing");
        stage.series = stageSeries;
        for (const agent of stage.agents) agent.series = [...stageSeries];
      });
      const strips = screen
        .getAllByTestId("token-lane")
        .map((lane) => within(lane).getByRole("img", { name: /last 30 min/ }));
      cleanup();
      return strips;
    };

    expect(cellCounts([0, 0, 7]).every((strip) => strip.children.length === 60)).toBe(true);
    expect(
      cellCounts(Array<number>(500).fill(3)).every((strip) => strip.children.length === 60),
    ).toBe(true);

    const [young] = cellCounts([0, 0, 7]);
    const levels = Array.from(young?.children ?? [], (cell) => cell.getAttribute("data-level"));
    expect(levels.slice(0, 57).every((level) => level === "0")).toBe(true);
    expect(levels[59]).toBe("4");
  });

  it("shows the four chips from the stage flow", () => {
    renderTokens("server");

    const section = screen.getByRole("region", { name: "stage tokens" });
    expect(within(section).getByText("43.2k/m")).toBeTruthy();
    expect(within(section).getByText("2.23M")).toBeTruthy();
    expect(within(section).getByText("93.1%")).toBeTruthy();
    expect(within(section).getByText("3 of 3")).toBeTruthy();
  });

  it("labels the legend with the shared lane scale and updates it with the snapshot", () => {
    const { store, raw } = renderTokens("server", (snapshot) => {
      const stage = snapshot.status.tokens.stages[0];
      if (!stage) throw new Error("fixture server stage is missing");
      stage.series = [1200];
      for (const agent of stage.agents) agent.series = [800];
    });

    const legend = screen.getByRole("img", {
      name: "intensity, 0 to 1200 fresh tokens per 30 s",
    });
    expect(legend.previousElementSibling?.textContent).toBe("0");
    expect(legend.nextElementSibling?.textContent).toBe("1.2k");
    for (const label of [legend.previousElementSibling, legend.nextElementSibling]) {
      expect(label?.classList).toContain("font-mono");
      expect(label?.classList).toContain("tabular-nums");
    }

    const next = structuredClone(raw);
    // applySnapshot rejects equal or older timestamps as repeated frames.
    next.generated_at = new Date(Date.parse(raw.generated_at) + 1000).toISOString();
    const stage = next.status.tokens.stages[0];
    const agent = stage?.agents[0];
    if (!stage || !agent) throw new Error("fixture server stage or agent is missing");
    stage.series = [2200];
    agent.series = [12500];
    act(() => applySnapshot(store, snapshotSchema.parse(next)));

    expect(screen.getByRole("img", { name: "intensity, 0 to 12500 fresh tokens per 30 s" })).toBe(
      legend,
    );
    expect(legend.previousElementSibling?.textContent).toBe("0");
    expect(legend.nextElementSibling?.textContent).toBe("12.5k");
  });

  it("shows zero on both ends of the legend when every series is zero", () => {
    renderTokens("server", (raw) => {
      const stage = raw.status.tokens.stages[0];
      if (!stage) throw new Error("fixture server stage is missing");
      stage.series = [0];
      for (const agent of stage.agents) agent.series = [0];
    });

    const legend = screen.getByRole("img", {
      name: "intensity, 0 to 0 fresh tokens per 30 s",
    });
    expect(legend.previousElementSibling?.textContent).toBe("0");
    expect(legend.nextElementSibling?.textContent).toBe("0");
  });

  it("draws one trend under each chip, named for its statistic", () => {
    renderTokens("server");

    const section = screen.getByRole("region", { name: "stage tokens" });
    for (const label of [
      "fresh rate, last 30 min",
      "fresh so far, last 30 min",
      "cache hit per 30 s, last 30 min",
      "agents live, last 30 min",
    ]) {
      expect(within(section).getAllByRole("img", { name: label })).toHaveLength(1);
    }
  });

  it("reads idle and the rate dash from the active flag, not the series", () => {
    renderTokens("server", (raw) => {
      const agent = raw.status.tokens.stages[0]?.agents[2];
      if (!agent) throw new Error("fixture codex agent is missing");
      agent.active = false;
      agent.series = [];
    });

    const lane = screen.getAllByTestId("token-lane")[2];
    expect(lane?.textContent).not.toMatch(/\b(live|idle)\b/);
    expect(lane?.textContent).toContain("—");
    const dot = within(lane as HTMLElement).getByRole("img", { name: "idle" });
    expect(dot.dataset.state).toBe("idle");
    expect(dot.classList).not.toContain("dot-live");
    expect(screen.getByText("2 of 3")).toBeTruthy();
  });

  it("renders a 200k-value series without throwing", () => {
    const long = Array<number>(200_000).fill(0);
    long[199_999] = 5;

    expect(() =>
      renderTokens("server", (raw) => {
        const stage = raw.status.tokens.stages[0];
        if (!stage) throw new Error("fixture server stage is missing");
        stage.series = [...long];
        for (const agent of stage.agents) agent.series = [...long];
      }),
    ).not.toThrow();
    expect(screen.getAllByTestId("token-lane")).toHaveLength(3);
  });

  it("renders nothing for a stage without a flow", () => {
    renderTokens("not-a-stage");

    expect(screen.queryByRole("region", { name: "stage tokens" })).toBeNull();
    expect(screen.queryAllByTestId("token-lane")).toHaveLength(0);
  });
});
