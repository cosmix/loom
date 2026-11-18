import { expect, it } from "vitest";

import slashVectors from "@/plan/api/fixtures/slash-vectors.json";
import type { Attachment } from "@/plan/api/schema-chat";
import { commandsViewSchema, type CommandsView } from "@/plan/api/schema-session-commands";
import {
  matchingCommands,
  messageProblem,
  pendingArgumentHint,
  typedCommandName,
  withCommand,
} from "./message-problem";

const ready = commandsViewSchema.parse(slashVectors.tables.ready);
const loading = commandsViewSchema.parse(slashVectors.tables.loading);
const unavailable = commandsViewSchema.parse(slashVectors.tables.unavailable);
const ALONE = "Send a command alone, on one line";

const passage: Attachment = {
  kind: "source",
  path: "doc/product/note.md",
  quote: "Selected passage",
  anchor: null,
  lines: null,
  stage: null,
  summary_sentence: null,
  rests_on: [],
  locked: null,
  changed_since_selected: false,
};

const names = (entries: CommandsView["entries"]) => entries.map((entry) => entry.name);

it("plain text is sent whatever the state of the command list", () => {
  for (const commands of [ready, loading, unavailable]) {
    expect(messageProblem("plain text", [], commands)).toBeNull();
    expect(messageProblem("plain text", [passage], commands)).toBeNull();
    expect(messageProblem("see /clear and doc/plans", [], commands)).toBeNull();
  }
  // An empty list (the route never answered) blocks slash messages only.
  const nothing: CommandsView = {
    state: "unavailable",
    generation: null,
    reason: null,
    entries: [],
  };
  expect(messageProblem("plain text", [], nothing)).toBeNull();
  expect(messageProblem("/compact", [], nothing)).toBe(
    "This session's command list is unavailable",
  );
});

it("a slash message with an attachment is refused, before its name is looked up", () => {
  expect(messageProblem("/simplify", [], ready)).toBeNull();
  expect(messageProblem("/simplify", [passage], ready)).toBe(ALONE);
  expect(messageProblem("/clear", [passage], ready)).toBe(ALONE);
  expect(messageProblem("/nonesuch", [passage], loading)).toBe(ALONE);
});

it("a name is matched by its row first and by an alias second, case and all", () => {
  expect(messageProblem("/reset", [], ready)).toBe("Changes the session outside the planner");
  expect(messageProblem("/args x", [], ready)).toBeNull();
  expect(messageProblem("/Simplify", [], ready)).toBe("Not in this session's command list");
  expect(messageProblem("/", [], ready)).toBe("Not in this session's command list");
});

it("a terminal-only row without a reason of its own reads as not verified", () => {
  const entries = ready.entries.map((entry) =>
    entry.name === "help" ? { ...entry, reason: null } : entry,
  );
  expect(messageProblem("/help", [], { ...ready, entries })).toBe("Not verified in the planner");
});

it("the length limit counts characters after the trailing space is dropped", () => {
  const line = `/simplify ${"é".repeat(290)}`;
  expect([...line]).toHaveLength(300);
  expect(messageProblem(`${line}   `, [], ready)).toBeNull();
  expect(messageProblem(`${line}x`, [], ready)).toBe("Too long for a command");
  expect(messageProblem("/simplify a\nb", [], ready)).toBe(ALONE);
  expect(messageProblem("/simplify a\n", [], ready)).toBeNull();
});

it("the typed name is the text after the slash until white space follows it", () => {
  expect(typedCommandName("/")).toBe("");
  expect(typedCommandName("  /com")).toBe("com");
  expect(typedCommandName(`${String.fromCodePoint(1)}/com`)).toBe("com");
  expect(typedCommandName("/compact ")).toBeNull();
  expect(typedCommandName("/compact\n")).toBeNull();
  expect(typedCommandName("a /compact")).toBeNull();
  expect(typedCommandName("")).toBeNull();
});

it("matching keeps the list's order, names and aliases that start with the text first", () => {
  expect(matchingCommands(ready, "")).toBe(ready.entries);
  expect(names(matchingCommands(ready, "co"))).toEqual(["context", "compact", "usage"]);
  expect(names(matchingCommands(ready, "ARGS"))).toEqual(["loom-plan:args"]);
  expect(names(matchingCommands(ready, "e"))).toEqual([
    "effort",
    "probe-personal",
    "context",
    "clear",
    "usage",
    "help",
  ]);
  expect(matchingCommands(ready, "zzz")).toEqual([]);
});

it("picking a command keeps what the draft already said as its arguments", () => {
  expect(withCommand("/", "simplify")).toBe("/simplify ");
  expect(withCommand("/sim", "simplify")).toBe("/simplify ");
  expect(withCommand("/sim src/plan", "simplify")).toBe("/simplify src/plan");
  expect(withCommand("  tidy the gate ", "simplify")).toBe("/simplify tidy the gate");
  expect(withCommand("", "context")).toBe("/context ");
});

it("the argument hint shows only while the named command has no argument yet", () => {
  const hint = "<optional custom summarization instructions>";
  expect(pendingArgumentHint("/compact ", ready)).toBe(hint);
  expect(pendingArgumentHint("/compact", ready)).toBeNull();
  expect(pendingArgumentHint("/compact keep", ready)).toBeNull();
  expect(pendingArgumentHint("/context ", ready)).toBeNull();
  expect(pendingArgumentHint("/nonesuch ", ready)).toBeNull();
});
