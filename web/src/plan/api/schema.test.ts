import { describe, expect, it } from "vitest";
import authoring from "@/plan/api/fixtures/authoring.json";
import checklist from "@/plan/api/fixtures/checklist.json";
import clientReading from "@/plan/api/fixtures/client-reading.json";
import clientSetName from "@/plan/api/fixtures/client-set-name.json";
import hello from "@/plan/api/fixtures/hello.json";
import presence from "@/plan/api/fixtures/presence.json";
import pressureRun from "@/plan/api/fixtures/pressure-run.json";
import refused from "@/plan/api/fixtures/refused.json";
import runOutput from "@/plan/api/fixtures/run-output.json";
import runState from "@/plan/api/fixtures/run-state.json";
import runStrip from "@/plan/api/fixtures/run-strip.json";
import serverInfo from "@/plan/api/fixtures/server-info.json";
import share from "@/plan/api/fixtures/share.json";
import {
  CLIENT_MESSAGE_TYPES,
  CURRENT_KINDS,
  PLAN_MESSAGE_TYPES,
  authoringViewSchema,
  checklistViewSchema,
  clientMessageSchema,
  currentViewSchema,
  planMessageSchema,
  presenceListSchema,
  refusedSchema,
  serverInfoSchema,
  shareLinkSchema,
  socketMessageSchema,
} from "./schema";

describe("planning wire schemas", () => {
  const fixtures = [
    ["authoring", authoringViewSchema, authoring],
    ["checklist", checklistViewSchema, checklist],
    ["client reading", clientMessageSchema, clientReading],
    ["client set name", clientMessageSchema, clientSetName],
    ["hello", planMessageSchema, hello],
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
        ...checklist,
        current: { ...checklist.current, extra: true },
      }).success,
    ).toBe(false);
  });

  it("accepts each flattened server message and nullable wire fields", () => {
    const messages = [
      { type: "presence", ...presence },
      { type: "authoring", holder: null },
      { type: "checklist", ...checklist, current: { kind: "ready" }, primary: null },
      { type: "checklist", ...checklist, current: { kind: "ready_all_done" } },
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
    ];
    for (const frame of frames) expect(socketMessageSchema.safeParse(frame).success).toBe(true);
  });

  it("rejects numbers outside Rust integer ranges", () => {
    expect(serverInfoSchema.safeParse({ ...serverInfo, port: 65536 }).success).toBe(false);
    expect(
      presenceListSchema.safeParse({ entries: [{ ...presence.entries[0], tabs: -1 }] }).success,
    ).toBe(false);
    expect(
      checklistViewSchema.safeParse({ ...checklist, current: { kind: "step", number: 256 } })
        .success,
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
