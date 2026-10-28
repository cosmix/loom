import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import type { Attachment } from "@/plan/api/schema-chat";

import { deliveredBytes, formatMessage } from "./format-attachments";

const FIXTURES = join(
  import.meta.dirname,
  "../../../../loom/tests/fixtures/plan_session/attachments",
);
const cases = readdirSync(FIXTURES)
  .filter((name) => name.endsWith(".json") && !name.startsWith("resolve-"))
  .sort();

function attachment(fields: Partial<Attachment> = {}): Attachment {
  return {
    kind: "source",
    path: "doc/test.md",
    anchor: null,
    lines: null,
    stage: null,
    summary_sentence: null,
    rests_on: [],
    quote: "",
    changed_since_selected: false,
    locked: null,
    ...fields,
  };
}

function block(quote: string): string {
  return `\n\n<loom-attachments>\n[1] source doc/test.md\n${quote}\n</loom-attachments>`;
}

describe("shared Rust attachment fixtures", () => {
  it("includes every formatter fixture", () => {
    expect(cases.length).toBeGreaterThanOrEqual(10);
  });

  it.each(cases)("formats %s byte for byte", (name) => {
    const input = JSON.parse(readFileSync(join(FIXTURES, name), "utf8")) as {
      text: string;
      attachments: Attachment[];
    };
    const expected = readFileSync(join(FIXTURES, name.replace(/\.json$/, ".txt")), "utf8");

    expect(formatMessage(input.text, input.attachments)).toBe(expected);
  });
});

describe("formatMessage fields", () => {
  it.each([
    ["", ""],
    ["first\r\nsecond\r\n", "first\r\nsecond\r\n"],
    ["<loom-attachments>\r\n", "\\<loom-attachments>\r\n"],
    ["\\\\</loom-attachments>", "\\\\\\</loom-attachments>"],
    [" \t\u0000<loom-attach\u0001ments>\r", " \t\u0000\\<loom-attach\u0001ments>\r"],
    ["\u0085<loom-attachments>\u0085", "\u0085\\<loom-attachments>\u0085"],
    ["\ufeff<loom-attachments>", "\ufeff<loom-attachments>"],
    ["text <loom-attachments>", "text <loom-attachments>"],
    ["<loom-\tattachments>", "<loom-\tattachments>"],
  ])("escapes only Rust tag lines without attachments: %j", (text, expected) => {
    expect(formatMessage(text, [])).toBe(expected);
  });

  it("orders optional fields and gives stage addresses precedence", () => {
    const attached = attachment({
      stage: "web",
      anchor: "ignored",
      summary_sentence: "s-12345678",
      lines: [2, 4],
      rests_on: ["doc/a.md#one", "doc/b.md#two"],
      locked: "running",
      changed_since_selected: true,
      quote: "passage",
    });

    expect(formatMessage("Go", [attached])).toBe(
      "Go\n\n<loom-attachments>\n[1] source doc/test.md#stage:web lines 2-4" +
        " rests-on doc/a.md#one,doc/b.md#two locked=running changed-since-selected\n" +
        "> passage\n</loom-attachments>",
    );
  });

  it("uses a summary sentence when the anchor is absent", () => {
    const attached = attachment({ kind: "summary", summary_sentence: "s-12345678" });

    expect(formatMessage("", [attached])).toBe(
      "\n\n<loom-attachments>\n[1] summary doc/test.md#s-12345678\n> \n</loom-attachments>",
    );
  });
});

describe("formatMessage quotes", () => {
  it("keeps a quote of exactly 2,000 Unicode scalar values", () => {
    const quote = "😀".repeat(2_000);

    expect(formatMessage("", [attachment({ quote })])).toBe(block(`> ${quote}`));
  });

  it("cuts a long first line on a Unicode scalar boundary", () => {
    const quote = "😀".repeat(2_001);

    expect(formatMessage("", [attachment({ quote })])).toBe(
      block(`> ${"😀".repeat(2_000)}\n> […1 more characters; read the source]`),
    );
  });

  it("counts newlines when cutting at a line boundary", () => {
    const first = "😀".repeat(1_999);
    const quote = `${first}\nx\ny`;

    expect(formatMessage("", [attachment({ quote })])).toBe(
      block(`> ${first}\n> […4 more characters; read the source]`),
    );
  });

  it("preserves a quote already ending in a cut marker", () => {
    const quote = `${"😀".repeat(2_001)}\n[…9 more characters; read the source]`;

    expect(formatMessage("", [attachment({ quote })])).toBe(
      block(`> ${"😀".repeat(2_001)}\n> […9 more characters; read the source]`),
    );
  });
});

it("deliveredBytes counts UTF-8 bytes", () => {
  expect(deliveredBytes("", [])).toBe(0);
  expect(deliveredBytes("é😀", [])).toBe(6);
  expect(deliveredBytes("<loom-attachments>", [])).toBe(19);
  expect(deliveredBytes("é😀", [attachment({ quote: "é😀" })])).toBe(78);
});
