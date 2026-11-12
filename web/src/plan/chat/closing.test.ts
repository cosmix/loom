import { describe, expect, it } from "vitest";

import type { Item, Turn } from "@/plan/api/schema-chat";
import {
  closingTicks,
  doneLists,
  endedByAgent,
  isClosingTurn,
  leftBehind,
  userWrote,
} from "./closing";
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

  it("an over-long decision or question refuses the whole block", () => {
    const only = (body: unknown) =>
      leftBehind([turn([user("closing"), block(JSON.stringify(body))])]);

    expect(only({ decisions: ["x".repeat(500)], questions: [] }).decisions).toHaveLength(1);
    expect(only({ decisions: ["x".repeat(501)], questions: ["Fine"] })).toMatchObject({
      decisions: [],
      questions: [],
    });
    expect(only({ decisions: ["Fine"], questions: ["x".repeat(501)] })).toMatchObject({
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

describe("the loom-done fence", () => {
  const lists = { decisions: ["Keep it small"], questions: ["Who owns it?"] };
  const fenced = (body: unknown, info = "loom-done", after = "\n") =>
    `Finished.\n\n${tick}${info}\n${typeof body === "string" ? body : JSON.stringify(body)}\n${tick}${after}`;
  const reply = (markdown: string) => item("text", { id: "reply", markdown });
  const ending = (markdown: string, id = "t1") =>
    turn([user(null), reply(markdown)], { id, open: false });

  it("a fence at the end of the last text gives its trimmed lists", () => {
    const padded = { decisions: ["  Keep it small  ", " "], questions: ["Who owns it?"] };

    expect(doneLists(ending(fenced(padded)))).toEqual(lists);
    expect(doneLists(ending(fenced(lists, "loom-done", "")))).toEqual(lists);
    expect(doneLists(ending(fenced(lists, "loom-done", "\n\n  \n")))).toEqual(lists);
  });

  it("text after the fence, a longer info string or a different one means no fence", () => {
    expect(doneLists(ending(`${fenced(lists)}\nOne more thing.`))).toBeNull();
    expect(doneLists(ending(fenced(lists, "loom-done-later")))).toBeNull();
    expect(doneLists(ending(fenced(lists, "loom-done x")))).toBeNull();
    expect(doneLists(ending(fenced(lists, "loom-closing")))).toBeNull();
    expect(doneLists(ending(`${tick}loom-done\n${JSON.stringify(lists)}`))).toBeNull();
  });

  it("only the final block counts, and only in the last text of the turn", () => {
    const quoted = `${fenced({ decisions: ["Old"], questions: [] })}\nAnd again:\n${fenced(lists)}`;
    expect(doneLists(ending(quoted))?.decisions).toEqual(["Keep it small"]);

    const earlier = turn([user(null), reply(fenced(lists)), reply("Anything else?")], {
      open: false,
    });
    expect(doneLists(earlier)).toBeNull();
    expect(doneLists(undefined)).toBeNull();
  });

  it("a malformed body, a wrong shape or an over-long list refuses the whole block", () => {
    expect(doneLists(ending(fenced("{not json")))).toBeNull();
    expect(doneLists(ending(fenced({ decisions: "no", questions: [] })))).toBeNull();
    expect(doneLists(ending(fenced({ decisions: ["Fine"] })))).toBeNull();
    expect(doneLists(ending(fenced({ decisions: Array(51).fill("x"), questions: [] })))).toBeNull();
    expect(doneLists(ending(fenced({ decisions: Array(50).fill("x"), questions: [] })))).not.toBe(
      null,
    );
    expect(doneLists(ending(fenced({ decisions: ["x".repeat(501)], questions: [] })))).toBeNull();
    expect(doneLists(ending(fenced({ decisions: ["x".repeat(500)], questions: [] })))).not.toBe(
      null,
    );
  });

  it("a user item holding the fence is ignored", () => {
    const quoting = item("user", {
      id: "u-fence",
      text: fenced(lists),
      origin: null,
      attachments: [],
      delivery: "delivered",
    });
    const quoted = turn([quoting], { open: false });

    expect(doneLists(quoted)).toBeNull();
    expect(endedByAgent([quoted])).toBe(false);
    expect(leftBehind([quoted])).toMatchObject({ decisions: [], questions: [] });
  });

  it("leftBehind falls back to the fence when there is no closing turn", () => {
    const talk = turn([user(null), edit("doc/a.md", "done", { added: 1, removed: 0 })], {
      id: "t0",
      open: false,
      changed_files: changed("doc/a.md"),
    });
    const left = leftBehind([talk, ending(fenced(lists), "t1")]);

    expect(left.documents).toEqual([{ path: "doc/a.md", added: 1, removed: 0 }]);
    expect(left).toMatchObject(lists);
  });

  it("a closing turn wins over a fence in the last turn", () => {
    const closed = turn([
      user("closing"),
      block(JSON.stringify({ decisions: ["Closing"], questions: [] })),
    ]);
    const left = leftBehind([closed, ending(fenced(lists), "t2")]);

    expect(left.decisions).toEqual(["Closing"]);
  });

  it("endedByAgent needs a valid fence in the last turn and no closing turn", () => {
    const fence = ending(fenced(lists));
    const plain = ending("Finished.", "t0");
    const closing = turn([user("closing"), reply("Done.")], { id: "t2", open: false });

    expect(endedByAgent([plain, fence])).toBe(true);
    expect(endedByAgent([fence, plain])).toBe(false);
    expect(endedByAgent([plain])).toBe(false);
    expect(endedByAgent([fence, closing])).toBe(false);
    expect(endedByAgent([ending(fenced("{not json"))])).toBe(false);
    expect(endedByAgent([])).toBe(false);
  });
});
