import { describe, expect, it } from "vitest";

import type { Item, Turn } from "@/plan/api/schema-chat";
import { closingTicks, isClosingTurn, leftBehind, userWrote } from "./closing";
import { item, turn } from "./transcript-test-kit";

const tick = "`".repeat(3);
type Delivery = "sent" | "delivered" | "undelivered";
const user = (origin: "closing" | "offer" | null, delivery: Delivery = "delivered"): Item =>
  item("user", { id: `u-${origin}-${delivery}`, text: "x", origin, attachments: [], delivery });
const edit = (
  path: string,
  status: "pending" | "done" | "error",
  fields: { name?: string; added?: number | null; removed?: number | null; id?: string } = {},
): Item =>
  item("tool", {
    id: fields.id ?? `${path}-${status}`,
    tool_use_id: fields.id ?? `${path}-${status}`,
    name: fields.name ?? "Edit",
    status,
    summary: path,
    added: fields.added ?? null,
    removed: fields.removed ?? null,
  });
const block = (body: string) =>
  item("text", {
    id: "reply",
    markdown: `Closing write done.\n\n${tick}loom-closing\n${body}\n${tick}`,
  });
const changed = (...paths: string[]): Turn["changed_files"] =>
  paths.map((path) => ({ path, before_hash: "h", by_tool: true }));

describe("isClosingTurn and userWrote", () => {
  it("a closing turn holds a user item with the closing origin", () => {
    expect(isClosingTurn(turn([user("closing")]))).toBe(true);
    expect(isClosingTurn(turn([user(null), user("offer")]))).toBe(false);
  });

  it("only a delivered or sent message of the user's own counts as written", () => {
    expect(userWrote([turn([user(null)])])).toBe(true);
    expect(userWrote([turn([user(null, "sent")])])).toBe(true);
    expect(userWrote([turn([user(null, "undelivered")])])).toBe(false);
    expect(userWrote([turn([user("offer"), user("closing")])])).toBe(false);
    expect(userWrote([])).toBe(false);
  });
});

describe("closingTicks", () => {
  it("lists each written document once, in order, done once any write of it is done", () => {
    const closing = turn([
      user("closing"),
      edit("doc/a.md", "pending", { id: "1" }),
      edit("doc/b.md", "done", { name: "Write", id: "2" }),
      edit("doc/a.md", "done", { id: "3" }),
      edit("doc/c.md", "pending", { name: "MultiEdit", id: "4" }),
      edit("doc/a.md", "pending", { id: "5" }),
    ]);

    expect(closingTicks(closing)).toEqual([
      { path: "doc/a.md", state: "done" },
      { path: "doc/b.md", state: "done" },
      { path: "doc/c.md", state: "live" },
    ]);
  });

  it("ignores reads, failed writes and other tools", () => {
    const closing = turn([
      user("closing"),
      edit("doc/a.md", "done", { name: "Read" }),
      edit("doc/b.md", "error"),
    ]);

    expect(closingTicks(closing)).toEqual([]);
  });
});

describe("leftBehind", () => {
  const talk = turn([user(null), edit("doc/a.md", "done", { added: 3, removed: 1, id: "t1" })], {
    id: "t1",
    open: false,
    changed_files: changed("doc/a.md"),
  });
  const reply = block(
    JSON.stringify({
      decisions: ["  Keep it small  ", "", "Ship it"],
      questions: ["Who owns it?"],
    }),
  );
  const closing = turn(
    [
      user("closing"),
      edit("doc/a.md", "done", { added: 2, removed: 0, id: "c1" }),
      edit("doc/b.md", "done", { id: "c2" }),
      reply,
    ],
    { id: "t2", open: false, changed_files: changed("doc/b.md", "doc/a.md") },
  );

  it("unions changed documents in first-seen order and sums the counts of done writes", () => {
    const left = leftBehind([talk, closing]);

    expect(left.documents).toEqual([
      { path: "doc/a.md", added: 5, removed: 1 },
      { path: "doc/b.md", added: null, removed: null },
    ]);
  });

  it("reads trimmed decisions and questions from the closing block", () => {
    const left = leftBehind([talk, closing]);

    expect(left.decisions).toEqual(["Keep it small", "Ship it"]);
    expect(left.questions).toEqual(["Who owns it?"]);
  });

  it("takes the last closing block of the closing turn", () => {
    const first = block(JSON.stringify({ decisions: ["Old"], questions: [] }));
    const again = turn([user("closing"), first, { ...reply, id: "later" }], { open: false });

    expect(leftBehind([again]).decisions).toEqual(["Keep it small", "Ship it"]);
  });

  it("a malformed block, a wrong shape or a missing block gives empty lists", () => {
    const only = (items: Item[]) => leftBehind([turn([user("closing"), ...items])]);

    expect(only([block("{not json")])).toMatchObject({
      decisions: [],
      questions: [],
    });
    expect(only([block('{"decisions":"no","questions":[]}')]).decisions).toEqual([]);
    expect(
      only([block(JSON.stringify({ decisions: Array(51).fill("x"), questions: [] }))]),
    ).toMatchObject({ decisions: [], questions: [] });
    expect(only([item("text", { id: "plain", markdown: "Closing write done." })])).toMatchObject({
      decisions: [],
      questions: [],
    });
  });

  it("a chat with no closing turn still lists its documents", () => {
    const left = leftBehind([talk]);

    expect(left.documents).toEqual([{ path: "doc/a.md", added: 3, removed: 1 }]);
    expect(left.decisions).toEqual([]);
  });
});
