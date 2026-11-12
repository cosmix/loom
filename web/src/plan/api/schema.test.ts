import { describe, expect, it } from "vitest";
import authoring from "@/plan/api/fixtures/authoring.json";
import background from "@/plan/api/fixtures/background.json";
import checklist from "@/plan/api/fixtures/checklist.json";
import clientReading from "@/plan/api/fixtures/client-reading.json";
import clientSetName from "@/plan/api/fixtures/client-set-name.json";
import draftTeam from "@/plan/api/fixtures/draft-team.json";
import hello from "@/plan/api/fixtures/hello.json";
import kickoff from "@/plan/api/fixtures/kickoff.json";
import presence from "@/plan/api/fixtures/presence.json";
import pressureRun from "@/plan/api/fixtures/pressure-run.json";
import refused from "@/plan/api/fixtures/refused.json";
import runOutput from "@/plan/api/fixtures/run-output.json";
import runState from "@/plan/api/fixtures/run-state.json";
import runStrip from "@/plan/api/fixtures/run-strip.json";
import serverInfo from "@/plan/api/fixtures/server-info.json";
import sessionCommands from "@/plan/api/fixtures/session-commands.json";
import share from "@/plan/api/fixtures/share.json";
import {
  CLIENT_MESSAGE_TYPES,
  CURRENT_KINDS,
  PLAN_MESSAGE_TYPES,
  authoringViewSchema,
  checklistViewSchema,
  clientMessageSchema,
  currentViewSchema,
  offerViewSchema,
  planMessageSchema,
  presenceListSchema,
  refusedSchema,
  serverInfoSchema,
  shareLinkSchema,
  socketMessageSchema,
} from "./schema";
import { commandsViewSchema } from "./schema-session-commands";

const commandsFrame = { type: "commands", session: "s1", ...sessionCommands };

describe("planning wire schemas", () => {
  it("accepts a nullable blocked reason on an offer and treats its absence as open", () => {
    const offer = checklist.checklists[0]!.primary!;
    expect(offerViewSchema.parse(offer).blocked_reason).toBeNull();
    expect(
      offerViewSchema.parse({ ...offer, blocked_reason: "Settle the metaplan first" })
        .blocked_reason,
    ).toBe("Settle the metaplan first");
    const { blocked_reason: _omitted, ...missing } = offer;
    expect(offerViewSchema.parse(missing).blocked_reason).toBeUndefined();
    expect(offerViewSchema.safeParse({ ...offer, blocked_reason: false }).success).toBe(false);
  });

  const fixtures = [
    ["commands view", commandsViewSchema, sessionCommands],
    ["commands frame", planMessageSchema, commandsFrame],
    ["authoring", authoringViewSchema, authoring],
    ["background", planMessageSchema, background],
    ["checklist", planMessageSchema, checklist],
    ["client reading", clientMessageSchema, clientReading],
    ["client set name", clientMessageSchema, clientSetName],
    ["hello", planMessageSchema, hello],
    ["kickoff", planMessageSchema, kickoff],
    ["draft team", planMessageSchema, draftTeam],
    ["presence", presenceListSchema, presence],
    ["refused", refusedSchema, refused],
    ["server info", serverInfoSchema, serverInfo],
    ["share", shareLinkSchema, share],
  ] as const;

  it.each(fixtures)("parses %s and rejects extra fields", (_name, schema, fixture) => {
    expect(schema.safeParse(fixture).success).toBe(true);
    expect(schema.safeParse({ ...fixture, extra: true }).success).toBe(false);
  });

  it("rejects unknown message types and extra nested fields", () => {
    expect(planMessageSchema.safeParse({ ...hello, type: "unknown" }).success).toBe(false);
    expect(
      planMessageSchema.safeParse({
        ...hello,
        server: { ...hello.server, you: { ...hello.server.you, extra: true } },
      }).success,
    ).toBe(false);
    expect(
      checklistViewSchema.safeParse({
        ...checklist.checklists[0]!,
        current: { ...checklist.checklists[0]!.current, extra: true },
      }).success,
    ).toBe(false);
  });

  it("requires the card fields and nullable step counts on a checklist", () => {
    const view = checklist.checklists[0]!;
    expect(checklistViewSchema.safeParse(view).success).toBe(true);
    for (const field of [
      "has_product",
      "sections",
      "metaplans",
      "plan_rows",
      "plans",
      "untested",
    ] as const) {
      const { [field]: _omitted, ...rest } = view;
      expect(checklistViewSchema.safeParse(rest).success).toBe(false);
    }
    // The feature is named by its slug: a title on the wire is an unknown key.
    expect(checklistViewSchema.safeParse({ ...view, title: "Demo" }).success).toBe(false);
    const row = view.plan_rows[0]!;
    expect(
      checklistViewSchema.safeParse({ ...view, plan_rows: [{ ...row, pressure_date: null }] })
        .success,
    ).toBe(true);
    expect(
      checklistViewSchema.safeParse({ ...view, plan_rows: [{ ...row, extra: true }] }).success,
    ).toBe(false);
    const [step, ...others] = view.steps;
    const withoutCount = { ...step!, count: undefined };
    expect(
      checklistViewSchema.safeParse({ ...view, steps: [withoutCount, ...others] }).success,
    ).toBe(false);
    expect(
      checklistViewSchema.safeParse({ ...view, steps: [{ ...step!, count: null }, ...others] })
        .success,
    ).toBe(true);
  });

  it("accepts each flattened server message and nullable wire fields", () => {
    const messages = [
      { type: "presence", ...presence },
      { type: "authoring", holder: null },
      {
        ...checklist,
        checklists: [{ ...checklist.checklists[0]!, current: { kind: "ready" }, primary: null }],
      },
      {
        ...checklist,
        checklists: [{ ...checklist.checklists[0]!, current: { kind: "ready_all_done" } }],
      },
      { type: "server_stopping" },
      { type: "refused", ...refused },
    ];
    for (const message of messages) expect(planMessageSchema.safeParse(message).success).toBe(true);
    expect(clientMessageSchema.safeParse({ type: "reading", path: null }).success).toBe(true);
  });

  it("parses each app-handoff frame with its type tag", () => {
    const frames = [
      { type: "pressure_run", ...pressureRun },
      { type: "run_state", ...runState },
      { type: "run_output", ...runOutput },
      { type: "run_strip", ...runStrip },
      kickoff,
      draftTeam,
    ];
    for (const frame of frames) expect(socketMessageSchema.safeParse(frame).success).toBe(true);
  });

  it("rejects numbers outside Rust integer ranges", () => {
    expect(serverInfoSchema.safeParse({ ...serverInfo, port: 65536 }).success).toBe(false);
    expect(
      presenceListSchema.safeParse({ entries: [{ ...presence.entries[0], tabs: -1 }] }).success,
    ).toBe(false);
    expect(
      checklistViewSchema.safeParse({
        ...checklist.checklists[0]!,
        current: { kind: "step", number: 256 },
      }).success,
    ).toBe(false);
  });
});

// loom/src/plan_server/wire/tests_schema.rs compares the Rust enums with these
// exported arrays only. The unions below are what parses frames, so each must
// list exactly the tags its array lists, or a variant added to the array alone
// passes the Rust test while the app drops the frame.
describe("schema unions track the exported tag arrays", () => {
  const sorted = (tags: readonly string[]) => [...tags].sort();

  it("lists every server message type in planMessageSchema", () => {
    const tags = planMessageSchema.options.map((option) => option.shape.type.value);
    expect(sorted(tags)).toEqual(sorted(PLAN_MESSAGE_TYPES));
  });

  it("lists every client message type in clientMessageSchema", () => {
    const tags = clientMessageSchema.options.map((option) => option.shape.type.value);
    expect(sorted(tags)).toEqual(sorted(CLIENT_MESSAGE_TYPES));
  });

  it("lists every current-step kind in currentViewSchema", () => {
    const tags = currentViewSchema.options.map((option) => option.shape.kind.value);
    expect(sorted(tags)).toEqual(sorted(CURRENT_KINDS));
  });
});
