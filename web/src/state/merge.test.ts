import { describe, expect, it, vi } from "vitest";

import { dynamicPart, fixture, stateFrame, tickFrame } from "@/api/frames-kit";
import type { Snapshot } from "@/api/schema";
import { mergeState, mergeTick, mergeTokens } from "@/state/merge";

const AT = "2026-09-04T12:00:01Z";

function stageOf(snapshot: Snapshot, id: string) {
  const stage = snapshot.status.stages.find((candidate) => candidate.id === id);
  if (!stage) throw new Error(`no stage ${id}`);
  return stage;
}

describe("mergeState", () => {
  it("equals the modified snapshot for changed stages, status keys and top-level keys", () => {
    const modified = structuredClone(fixture);
    const server = stageOf(modified, "server");
    server.status = "blocked";
    server.held = true;
    server.elapsed_secs = 910;
    modified.status.plan_name = "Another plan";
    modified.status.progress.completed += 1;
    modified.alerts = [];
    modified.daemon = "not-running";
    modified.notice = "daemon lane degraded";
    modified.generated_at = AT;

    const frame = stateFrame({
      stages: { server: dynamicPart(server) },
      status: { plan_name: "Another plan", progress: modified.status.progress },
      alerts: [],
      daemon: "not-running",
      notice: "daemon lane degraded",
    });

    expect(mergeState(fixture, frame)).toEqual(modified);
  });

  it("drops an optional key the new dynamic part no longer carries", () => {
    const modified = structuredClone(fixture);
    delete stageOf(modified, "merge-gate").merge_block;
    expect(stageOf(fixture, "merge-gate").merge_block).toBeTruthy();

    const frame = stateFrame({
      stages: { "merge-gate": dynamicPart(stageOf(modified, "merge-gate")) },
    });
    const merged = stageOf(mergeState(fixture, frame), "merge-gate");

    expect("merge_block" in merged).toBe(false);
    expect(merged.name).toBe(stageOf(fixture, "merge-gate").name);
    expect(merged.dependencies).toBe(stageOf(fixture, "merge-gate").dependencies);
  });

  it("clears the notice on null and keeps it when absent", () => {
    const noisy = { ...structuredClone(fixture), notice: "degraded" };

    expect("notice" in mergeState(noisy, stateFrame({ notice: null }))).toBe(false);
    expect(mergeState(noisy, stateFrame({ alerts: [] })).notice).toBe("degraded");
  });

  it("leaves untouched stages and the previous snapshot alone", () => {
    const before = structuredClone(fixture);
    const frame = stateFrame({ stages: { server: dynamicPart(stageOf(fixture, "server")) } });

    const merged = mergeState(fixture, frame);

    expect(fixture).toEqual(before);
    expect(stageOf(merged, "docs")).toBe(stageOf(fixture, "docs"));
    expect(merged.attention).toBe(fixture.attention);
    expect(merged.generated_at).toBe(AT);
  });

  it("ignores a stage the snapshot does not hold", () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const frame = stateFrame({ stages: { ghost: dynamicPart(stageOf(fixture, "server")) } });

    const merged = mergeState(fixture, frame);

    expect(merged.status.stages).toEqual(fixture.status.stages);
    expect(error).toHaveBeenCalledWith("dashboard: state for an unknown stage", "ghost");
    error.mockRestore();
  });
});

describe("mergeTick", () => {
  it("equals the modified snapshot for moving clocks", () => {
    const modified = structuredClone(fixture);
    const server = stageOf(modified, "server");
    server.elapsed_secs = 906;
    server.execution_secs = 641;
    server.staleness_secs = 4;
    modified.tick_age_secs = 7;
    modified.generated_at = AT;

    const frame = tickFrame({
      tick_age: 7,
      stages: { server: { elapsed: 906, exec: 641, stale: 4 } },
    });

    expect(mergeTick(fixture, frame)).toEqual(modified);
  });

  it("keeps the object identity of everything it does not touch", () => {
    const before = structuredClone(fixture);

    const merged = mergeTick(fixture, tickFrame({ stages: { server: { elapsed: 906 } } }));

    expect(fixture).toEqual(before);
    expect(stageOf(merged, "server")).not.toBe(stageOf(fixture, "server"));
    for (const stage of fixture.status.stages) {
      if (stage.id !== "server") expect(stageOf(merged, stage.id)).toBe(stage);
    }
    merged.attention.forEach((entry, index) => expect(entry).toBe(fixture.attention[index]));
    expect(merged.status.merge).toBe(fixture.status.merge);
  });

  it("keeps the status object when no stage moved", () => {
    const merged = mergeTick(fixture, tickFrame({ tick_age: null }));

    expect(merged.status).toBe(fixture.status);
    expect(merged.attention).toBe(fixture.attention);
    expect(merged.tick_age_secs).toBeNull();
    expect(merged.generated_at).toBe(AT);
  });

  it("overlays the judge heartbeat on the stage and on its attention entry", () => {
    const prev = structuredClone(fixture);
    stageOf(prev, "docs").judge_heartbeat_secs = 10;
    prev.attention.find((entry) => entry.id === "docs")!.judge_heartbeat_secs = 10;

    const merged = mergeTick(
      prev,
      tickFrame({ stages: { docs: { judge: 11 }, client: { judge: 5 } } }),
    );
    const judgeOf = (id: string) =>
      merged.attention.find((entry) => entry.id === id)!.judge_heartbeat_secs;

    expect(stageOf(merged, "docs").judge_heartbeat_secs).toBe(11);
    expect(judgeOf("docs")).toBe(11);
    // "client" has an attention entry without a running judge: it stays null.
    expect(judgeOf("client")).toBeNull();
  });

  it("applies the contract clock only to a stage that has contract testing", () => {
    const prev = structuredClone(fixture);
    stageOf(prev, "design").contract_testing = {
      done: 1,
      total: 3,
      queued: false,
      running_secs: 5,
    };

    const merged = mergeTick(
      prev,
      tickFrame({ stages: { design: { contract: 6 }, server: { contract: 9 } } }),
    );

    expect(stageOf(merged, "design").contract_testing).toEqual({
      done: 1,
      total: 3,
      queued: false,
      running_secs: 6,
    });
    expect(stageOf(merged, "server").contract_testing).toBeNull();
  });
});

describe("mergeTokens", () => {
  it("removes the token flow and puts it back", () => {
    const without = structuredClone(fixture);
    delete without.status.tokens;

    const removed = mergeTokens(fixture, undefined);
    const restored = mergeTokens(removed, fixture.status.tokens);

    expect(removed).toEqual(without);
    expect("tokens" in removed.status).toBe(false);
    expect(restored).toEqual(fixture);
    expect(fixture.status.tokens).toBeDefined();
  });
});
