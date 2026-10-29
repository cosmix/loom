import { describe, expect, it } from "vitest";
import panel from "@/plan/api/fixtures/summary-panel.json";
import question from "@/plan/api/fixtures/summary-question.json";
import turnCost from "@/plan/api/fixtures/summary-turn-cost.json";
import {
  summaryPanelSchema,
  summaryQuestionSchema,
  summaryTurnCostSchema,
  summaryMessageSchemas,
} from "./schema-summaries";

describe("summary wire schemas", () => {
  it("parses the panel and preserves unknown token counts", () => {
    const parsed = summaryPanelSchema.parse(panel);
    expect(parsed).toEqual(panel);
    expect(parsed.last?.tokens.cache_write).toBeUndefined();
    expect(summaryPanelSchema.safeParse({ ...panel, today: { output: "unknown" } }).success).toBe(
      false,
    );
    expect(summaryPanelSchema.parse({ ...panel, today: {}, week: {} }).today).toEqual({});
  });

  it("parses the question and rejects an unknown estimate basis", () => {
    expect(summaryQuestionSchema.parse(question)).toEqual(question);
    expect(summaryQuestionSchema.safeParse({ ...question, basis: "unknown" }).success).toBe(false);
  });

  it("parses turn costs and preserves omitted tokens", () => {
    const parsed = summaryTurnCostSchema.parse(turnCost);
    expect(parsed).toEqual(turnCost);
    expect(parsed.rows[1].tokens).toBeUndefined();
    expect(
      summaryTurnCostSchema.safeParse({ ...turnCost, rows: [{ id: "x", tokens: null }] }).success,
    ).toBe(false);
  });

  it("parses flattened messages and rejects invalid tags", () => {
    const messages = [
      { type: "summary_panel", ...panel },
      { type: "summary_question", ...question },
      { type: "summary_question_closed", id: question.id },
      { type: "summary_turn_cost", ...turnCost },
    ];
    for (const [index, schema] of summaryMessageSchemas.entries()) {
      expect(schema.safeParse(messages[index]).success).toBe(true);
      expect(schema.safeParse({ ...messages[index], type: "unknown" }).success).toBe(false);
    }
  });
});
