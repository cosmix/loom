# Web C2: token display helpers (codex unit)

Read doc/plans/briefs/token-flow/common.md sections 2, 4 and 6 first. Do not run git. Do not touch any path under .loom. Do not run oxfmt.

## Files

- Write: web/src/lib/tokens.ts and web/src/lib/tokens.test.ts.
- Read only: web/src/lib/format.ts (the Tone type), web/src/api/schema.ts (types written in parallel by unit C1; import them with `import type` only, so this file loads before C1 lands).

## Surface (pinned; other workers import it)

    export function fresh(u: TokenUsage): number
    export function compactTokens(n: number): string
    export function cacheHitPercent(u: TokenUsage): number | null
    export function stageFlow(flow: TokenFlow | undefined, stageId: string): StageFlow | undefined
    export function agentLabel(a: AgentFlow): string
    export function quotaLeftText(q: QuotaLeft): { text: string; tone: Tone } | null
    export function modelRates(flow: TokenFlow): Array<{ model: string; perMinute: number }>

## Rules

- fresh is input + cache_write + output.
- compactTokens:
  - under 1000: the plain integer ("980");
  - under 100000: one decimal plus k, truncated, never rounded ("43.2k"; 99999 gives "99.9k");
  - under 1e6: whole k, truncated ("312k"; 999999 gives "999k");
  - otherwise: two decimals plus M, truncated ("2.23M").
  - Truncation (Math.floor on the scaled value) matches the TUI's compact_tokens and never yields "100.0k" or "1000k".
- cacheHitPercent = cache_read / (cache_read + input + cache_write) * 100, or null when the denominator is 0.
- agentLabel:
  - main gives "main session";
  - subagent gives agent_type or "subagent";
  - codex gives "codex job".
- quotaLeftText:
  - lasts_to_reset gives { "lasts to reset", "dimmed" };
  - minutes_left m >= 60 gives "~{h}h{mm}m left" (mm zero-padded), and m < 60 gives "~{m}m left";
  - the tone is "blocked" when m < 30, else "warning";
  - null minutes with lasts_to_reset false gives null.
- modelRates sums fresh(agent.rate) per model across all stages (null model grouped as "unknown"), sorted descending.

## Tests (tokens.test.ts)

Build the inputs from the common.md section 4 values:

- compactTokens at 0, 980, 43200, 99999, 312000, 999999 and 2230000;
- cacheHitPercent of the plan total rounds to 93.1;
- quotaLeftText for 127, 43, 20, lasts_to_reset, and null;
- agentLabel for the three fixture agents;
- modelRates gives claude-sonnet-5-5 25200, claude-opus-5-5 14200, gpt-6.1-sol 3800, in that order;
- stageFlow for "server" and for an unknown id.

Done when `bun run test src/lib/tokens` passes from web/. Write erasable TypeScript only: no parameter properties, enums or namespaces (tsconfig sets erasableSyntaxOnly; vitest accepts code that `tsc -b` rejects with TS1294). The orchestrator runs `bun run typecheck` after your wave.
