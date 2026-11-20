# Web C1: zod schema for StatusData.tokens (codex unit)

Read doc/plans/briefs/token-flow/common.md sections 3 and 4 first. Stage token-core has merged: web/src/api/fixtures/snapshot.json already carries status.tokens. Do not run git. Do not touch any path under .loom. Do not run oxfmt (the orchestrator formats).

## Files

- Write: web/src/api/schema.ts and web/src/api/schema.test.ts.
- Read only: web/src/api/fixtures/snapshot.json, web/src/api/zod.ts (import z from "@/api/zod", never from "zod").

## Steps

1. In schema.ts add, before statusDataSchema:
   - tokenUsageSchema = z.object({ input, cache_write, cache_read, output }), each z.number().int().nonnegative(), .strict();
   - agentFlowSchema with:
     - id string;
     - kind z.enum(["main","subagent","codex"]);
     - agent_type and model z.string().nullable();
     - active boolean;
     - total and rate tokenUsageSchema;
     - series z.array(z.number().int().nonnegative());
     - .strict();
   - stageFlowSchema with stage_id, total, rate, series and agents, .strict();
   - planFlowSchema with total, rate and series, .strict();
   - quotaLeftSchema with provider z.enum(["claude","codex"]), minutes_left z.number().int().nonnegative().nullable(), lasts_to_reset boolean, .strict();
   - tokenFlowSchema with observed_at and bucket_secs (int), plan, stages, quota_left and unreadable_lines (int, nonnegative), .strict().
2. Add `tokens: tokenFlowSchema.optional()` to statusDataSchema.
3. Export the types TokenUsage, AgentFlow, StageFlow, PlanFlow, QuotaLeft and TokenFlow via z.infer, next to the existing type exports.
4. Tests in schema.test.ts:
   - the snapshot fixture parses and status.tokens.plan.rate.cache_write equals 36000;
   - the server stage has three agents with kinds main, subagent and codex;
   - a snapshot without status.tokens still parses;
   - an agent with an unknown key is rejected;
   - a negative token count is rejected.

Done when `bun run test src/api` passes from web/. Keep functions short and match the file style. Write erasable TypeScript only: no parameter properties, enums or namespaces (tsconfig sets erasableSyntaxOnly; vitest accepts code that `tsc -b` rejects with TS1294). The orchestrator runs `bun run typecheck` after your wave.
