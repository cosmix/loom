import { describe, expect, it } from "vitest";
import draftTeam from "@/plan/api/fixtures/draft-team.json";
import kickoff from "@/plan/api/fixtures/kickoff.json";
import { planMessageSchema, socketCommandSchema } from "./schema";
import { chatWireCommandSchema } from "./schema-chat";
import {
  draftTeamViewSchema,
  kickoffShapeSchema,
  kickoffSizeSchema,
  kickoffViewSchema,
  shapeSectionSchema,
  shapeStateSchema,
  teammateStatusSchema,
  tokenCountsSchema,
} from "./schema-kickoff";

describe("kickoff wire fixtures", () => {
  it("parses and narrows the complete kickoff frame", () => {
    const message = planMessageSchema.parse(kickoff);

    expect(message).toEqual(kickoff);
    if (message.type !== "kickoff") throw new Error("Expected a kickoff frame");
    expect(message.shape.sections).toEqual([
      { title: "Problem", state: "settled" },
      { title: "Design", state: "wording" },
    ]);
    expect(message.target).toBe("doc/product/planner-ui/index.md");
  });

  it("parses and narrows the complete draft-team frame", () => {
    const message = planMessageSchema.parse(draftTeam);

    expect(message).toEqual(draftTeam);
    if (message.type !== "draft_team") throw new Error("Expected a draft-team frame");
    expect(message.teammates.map(({ state, current }) => ({ state, current }))).toEqual([
      { state: "working", current: "doc/product/planner-ui/section_01.md" },
      { state: "idle", current: null },
    ]);
  });

  it.each([kickoff, draftTeam])("rejects extra fields on a $type frame", (frame) => {
    expect(planMessageSchema.safeParse({ ...frame, extra: true }).success).toBe(false);
  });
});

describe("kickoff nested wire objects", () => {
  const { type: _kickoffType, ...kickoffView } = kickoff;
  const { type: _teamType, ...teamView } = draftTeam;
  const cases = [
    ["kickoff view", kickoffViewSchema, kickoffView],
    ["shape", kickoffShapeSchema, kickoff.shape],
    ["section", shapeSectionSchema, kickoff.shape.sections[0]],
    ["draft-team view", draftTeamViewSchema, teamView],
    ["teammate", teammateStatusSchema, draftTeam.teammates[0]],
    ["tokens", tokenCountsSchema, draftTeam.teammates[0].tokens],
  ] as const;

  it.each(cases)("rejects unknown fields in %s", (_name, schema, value) => {
    expect(schema.safeParse({ ...value, extra: true }).success).toBe(false);
  });

  it("rejects unknown fields nested inside a tagged frame", () => {
    const frame = {
      ...draftTeam,
      teammates: [
        {
          ...draftTeam.teammates[0],
          tokens: { ...draftTeam.teammates[0].tokens, extra: true },
        },
      ],
    };

    expect(planMessageSchema.safeParse(frame).success).toBe(false);
  });
});

describe("kickoff enum wire spellings", () => {
  it.each(["settled", "wording", "open"])("accepts shape state %s", (state) => {
    expect(shapeStateSchema.parse(state)).toBe(state);
  });

  it.each(["feature", "plan"])("accepts kickoff size %s", (size) => {
    expect(kickoffSizeSchema.parse(size)).toBe(size);
  });

  it("rejects a misspelled section state in a kickoff frame", () => {
    const frame = {
      ...kickoff,
      shape: { ...kickoff.shape, sections: [{ title: "Design", state: "Wording" }] },
    };

    expect(planMessageSchema.safeParse(frame).success).toBe(false);
  });

  it("rejects a misspelled teammate state in a draft-team frame", () => {
    const frame = {
      ...draftTeam,
      teammates: [{ ...draftTeam.teammates[0], state: "Working" }],
    };

    expect(planMessageSchema.safeParse(frame).success).toBe(false);
  });
});

describe("kickoff required nullable fields", () => {
  it("accepts null kickoff size, name and target", () => {
    const frame = {
      ...kickoff,
      shape: { ...kickoff.shape, size: null, name: null },
      target: null,
    };

    expect(planMessageSchema.parse(frame)).toEqual(frame);
  });

  const cases = [
    ["target", { ...kickoff, target: undefined }],
    ["size", { ...kickoff, shape: { ...kickoff.shape, size: undefined } }],
    ["name", { ...kickoff, shape: { ...kickoff.shape, name: undefined } }],
    ["current", { ...draftTeam, teammates: [{ ...draftTeam.teammates[0], current: undefined }] }],
  ] as const;

  it.each(cases)("requires the nullable %s field", (_name, frame) => {
    expect(planMessageSchema.safeParse(frame).success).toBe(false);
  });
});

describe("teammate token counts", () => {
  it.each(["input", "cache_write", "cache_read", "output"])(
    "rejects negative and fractional %s counts in a draft-team frame",
    (field) => {
      const frame = (value: number) => ({
        ...draftTeam,
        teammates: [
          {
            ...draftTeam.teammates[0],
            tokens: { ...draftTeam.teammates[0].tokens, [field]: value },
          },
        ],
      });

      expect(planMessageSchema.safeParse(frame(-1)).success).toBe(false);
      expect(planMessageSchema.safeParse(frame(0.5)).success).toBe(false);
    },
  );
});

describe("confirm_kickoff wire command", () => {
  const command = {
    type: "chat",
    command: "confirm_kickoff",
    client_id: "confirm-1",
    size: "feature",
    name: "planner-ui",
  };
  const schemas = [
    ["chat", chatWireCommandSchema],
    ["socket", socketCommandSchema],
  ] as const;

  it.each(schemas)("parses the command through the %s schema", (_name, schema) => {
    expect(schema.parse(command)).toEqual(command);
    const planCommand = { ...command, size: "plan", client_id: null };
    expect(schema.parse(planCommand)).toEqual(planCommand);
  });

  it.each(schemas)("requires a name through the %s schema", (_name, schema) => {
    const { name: _nameField, ...withoutName } = command;

    expect(schema.safeParse(withoutName).success).toBe(false);
  });

  it.each(schemas)("requires a size through the %s schema", (_name, schema) => {
    const { size: _size, ...withoutSize } = command;

    expect(schema.safeParse(withoutSize).success).toBe(false);
  });

  it.each(schemas)("rejects invalid fields through the %s schema", (_name, schema) => {
    expect(schema.safeParse({ ...command, extra: true }).success).toBe(false);
    expect(schema.safeParse({ ...command, size: "Feature" }).success).toBe(false);
    expect(schema.safeParse({ ...command, name: null }).success).toBe(false);
  });
});
