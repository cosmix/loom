import { describe, expect, it } from "vitest";
import authoring from "@/plan/api/fixtures/authoring.json";
import checklist from "@/plan/api/fixtures/checklist.json";
import clientReading from "@/plan/api/fixtures/client-reading.json";
import clientSetName from "@/plan/api/fixtures/client-set-name.json";
import hello from "@/plan/api/fixtures/hello.json";
import presence from "@/plan/api/fixtures/presence.json";
import refused from "@/plan/api/fixtures/refused.json";
import serverInfo from "@/plan/api/fixtures/server-info.json";
import share from "@/plan/api/fixtures/share.json";
import {
  authoringViewSchema,
  checklistViewSchema,
  clientMessageSchema,
  planMessageSchema,
  presenceListSchema,
  refusedSchema,
  serverInfoSchema,
  shareLinkSchema,
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
