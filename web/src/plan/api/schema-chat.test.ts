import { describe, expect, it } from "vitest";
import commands from "@/plan/api/fixtures/chat-commands.json";
import deltas from "@/plan/api/fixtures/chat-deltas.json";
import frames from "@/plan/api/fixtures/chat-frames.json";
import snapshot from "@/plan/api/fixtures/chat-snapshot.json";
import {
  attachmentInputSchema,
  attachmentKindSchema,
  attachmentSchema,
  chatCommandSchema,
  chatDeltaSchema,
  chatFrameSchema,
  chatSnapshotSchema,
  deliverySchema,
  denyReasonSchema,
  failureKindSchema,
  itemSchema,
  lockedTagSchema,
  noticeKindSchema,
  originSchema,
  permissionStatusSchema,
  sessionStateSchema,
  sessionViewSchema,
  tokenCountsSchema,
  toolStatusSchema,
  turnSchema,
} from "./schema-chat";

describe("chat wire fixtures", () => {
  it("parses the complete snapshot without losing fields", () => {
    expect(chatSnapshotSchema.parse(snapshot)).toEqual(snapshot);
  });

  it.each(deltas)("parses delta $op without losing fields", (delta) => {
    expect(chatDeltaSchema.parse(delta)).toEqual(delta);
  });

  it.each(commands)("parses command $command without losing fields", (command) => {
    expect(chatCommandSchema.parse(command)).toEqual(command);
  });

  it.each(frames)("parses frame $frame without losing fields", (frame) => {
    expect(chatFrameSchema.parse(frame)).toEqual(frame);
  });

  it.each(deltas)("parses flattened delta frame $op", (delta) => {
    const frame = { frame: "delta", ...delta };

    expect(chatFrameSchema.parse(frame)).toEqual(frame);
  });

  const items = snapshot.turns.flatMap<{ type: string; id: string }>((turn) => turn.items);
  it.each(items)("parses item $type ($id) without losing fields", (item) => {
    expect(itemSchema.parse(item)).toEqual(item);
  });

  it("covers every item type in the snapshot", () => {
    expect([...new Set(items.map((item) => item.type))].sort()).toEqual([
      "notice",
      "permission",
      "text",
      "thinking",
      "tool",
      "user",
    ]);
  });
});

describe("chat wire enums", () => {
  const cases = [
    [
      "SessionState",
      sessionStateSchema,
      ["starting", "idle", "working", "permission", "ending", "ended", "failed"],
    ],
    ["FailureKind", failureKindSchema, ["spawn", "died"]],
    ["Delivery", deliverySchema, ["sent", "delivered", "undelivered"]],
    ["Origin", originSchema, ["offer", "closing"]],
    ["ToolStatus", toolStatusSchema, ["pending", "done", "error", "denied", "not_finished"]],
    ["PermissionStatus", permissionStatusSchema, ["pending", "allowed", "denied"]],
    [
      "DenyReason",
      denyReasonSchema,
      [
        "holder",
        "timeout",
        "session_ended",
        "server_stopping",
        "hook_error",
        "interrupt",
        "summary_session",
      ],
    ],
    ["NoticeKind", noticeKindSchema, ["interrupted", "compacted", "api_error", "stopped", "info"]],
    ["AttachmentKind", attachmentKindSchema, ["source", "summary"]],
    ["LockedTag", lockedTagSchema, ["running", "done", "retired", "knowledge", "brief"]],
  ] as const;

  it.each(cases)("accepts every %s value and rejects unknown values", (_name, schema, values) => {
    expect(values.map((value) => schema.parse(value))).toEqual(values);
    expect(schema.safeParse("unknown").success).toBe(false);
  });
});

describe("chat wire boundaries", () => {
  it("requires nullable session fields while accepting null values", () => {
    const nullable = {
      ...snapshot.session,
      holder: null,
      away_until: null,
      working_since: null,
      last_activity: null,
      failure: null,
      offer: null,
    };
    const { holder: _holder, ...missingHolder } = nullable;

    expect(sessionViewSchema.parse(nullable)).toEqual(nullable);
    expect(sessionViewSchema.safeParse(missingHolder).success).toBe(false);
  });

  it("requires the command key on a user item and accepts a populated one", () => {
    const user = {
      type: "user",
      id: "u1",
      text: "/loom-plan:args x",
      attachments: [],
      delivery: "delivered",
      origin: null,
      command: null,
    };
    const command = { name: "loom-plan:args", args: "x" };
    const { command: _command, ...missing } = user;

    expect(itemSchema.parse({ ...user, command })).toEqual({ ...user, command });
    expect(itemSchema.safeParse(missing).success).toBe(false);
    expect(itemSchema.safeParse({ ...user, command: { name: "a" } }).success).toBe(false);
    expect(itemSchema.safeParse({ ...user, command: { ...command, extra: 1 } }).success).toBe(
      false,
    );
  });

  it("accepts an empty snapshot with no current or previous session", () => {
    const empty = { session: null, turns: [], queue: [], previous: null };

    expect(chatSnapshotSchema.parse(empty)).toEqual(empty);
  });

  it.each(commands)("rejects extra fields on command $command", (command) => {
    expect(chatCommandSchema.safeParse({ ...command, unexpected: true }).success).toBe(false);
  });

  it.each(frames)("rejects extra fields on frame $frame", (frame) => {
    expect(chatFrameSchema.safeParse({ ...frame, unexpected: true }).success).toBe(false);
  });
});

describe("chat wire tags and integer ranges", () => {
  it("rejects unknown discriminator values", () => {
    expect(chatFrameSchema.safeParse({ frame: "unknown" }).success).toBe(false);
    expect(chatFrameSchema.safeParse({ frame: "delta", op: "unknown" }).success).toBe(false);
    expect(chatDeltaSchema.safeParse({ op: "unknown" }).success).toBe(false);
    expect(itemSchema.safeParse({ type: "unknown" }).success).toBe(false);
    expect(chatCommandSchema.safeParse({ command: "unknown" }).success).toBe(false);
  });

  it("rejects wrapped payloads in place of flattened frame fields", () => {
    expect(chatFrameSchema.safeParse({ frame: "snapshot", snapshot }).success).toBe(false);
    expect(chatFrameSchema.safeParse({ frame: "delta", delta: deltas[0] }).success).toBe(false);
  });

  it.each([-1, 0.5, 4294967296])("rejects invalid u32 count %s", (queued) => {
    expect(sessionViewSchema.safeParse({ ...snapshot.session, queued }).success).toBe(false);
  });

  it("accepts the maximum u32 count", () => {
    expect(sessionViewSchema.parse({ ...snapshot.session, queued: 4294967295 }).queued).toBe(
      4294967295,
    );
  });

  it.each([-1, 0.5])("rejects invalid u64 count or duration %s", (value) => {
    expect(
      tokenCountsSchema.safeParse({ input: value, output: 0, cache_read: 0, cache_write: 0 })
        .success,
    ).toBe(false);
    expect(turnSchema.safeParse({ ...snapshot.turns[0], duration_ms: value }).success).toBe(false);
  });
});

describe("chat attachment wire boundaries", () => {
  const input = commands[0].attachments?.[0];
  it("preserves repo-relative attachment paths and rests_on addresses", () => {
    const attachment = {
      ...input,
      kind: "summary",
      path: "doc/loom/summaries/bridge.md",
      anchor: "s-1234abcd",
      summary_sentence: "s-1234abcd",
      rests_on: ["doc/plans/bridge.md#overview"],
    };

    expect(attachmentInputSchema.parse(attachment)).toEqual(attachment);
  });

  it("distinguishes input provenance from resolved attachment state", () => {
    const { selected_hash: _hash, ...shared } = attachmentInputSchema.parse(input);
    const resolved = { ...shared, changed_since_selected: true, locked: null };

    expect(attachmentSchema.parse(resolved)).toEqual(resolved);
    expect(attachmentSchema.safeParse(input).success).toBe(false);
    expect(attachmentInputSchema.safeParse(resolved).success).toBe(false);
  });

  it.each([[1], [1, 2, 3], [-1, 2], [1, 0.5], [1, 4294967296]])(
    "rejects malformed attachment line tuple %j",
    (...lines) => {
      expect(attachmentInputSchema.safeParse({ ...input, lines }).success).toBe(false);
    },
  );
});
