import type { StageSummary } from "@/api/schema";

export interface OrderedStage {
  stage: StageSummary;
  level: number;
}

/** Compute Rust-compatible dependency levels for all supplied stages. */
export function computeLevels(stages: readonly StageSummary[]): Map<string, number> {
  const stageById = new Map<string, StageSummary>();
  for (const stage of stages) {
    stageById.set(stage.id, stage);
  }

  const levels = new Map<string, number>();
  const compute = (id: string, visiting: Set<string>): number => {
    const cached = levels.get(id);
    if (cached !== undefined) {
      return cached;
    }
    if (visiting.has(id)) {
      return 0;
    }

    visiting.add(id);
    const stage = stageById.get(id);
    if (!stage) {
      return 0;
    }

    const level =
      stage.dependencies.length === 0
        ? 0
        : Math.max(...stage.dependencies.map((dependency) => compute(dependency, visiting))) + 1;
    visiting.delete(id);
    levels.set(id, level);
    return level;
  };

  for (const stage of stages) {
    compute(stage.id, new Set());
  }
  return levels;
}

/** Deduplicate by id, retaining the first stage, then order by level and id. */
export function orderStages(stages: readonly StageSummary[]): OrderedStage[] {
  const levels = computeLevels(stages);
  const firstById = new Map<string, StageSummary>();
  for (const stage of stages) {
    if (!firstById.has(stage.id)) {
      firstById.set(stage.id, stage);
    }
  }

  return [...firstById.values()]
    .map((stage) => ({ stage, level: levels.get(stage.id) ?? 0 }))
    .sort((left, right) => {
      if (left.level !== right.level) {
        return left.level - right.level;
      }
      return left.stage.id < right.stage.id ? -1 : left.stage.id > right.stage.id ? 1 : 0;
    });
}
