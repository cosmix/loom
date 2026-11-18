import { expect, it } from "vitest";

import { quoteLines } from "@/plan/chat/format-attachments";
import { docAttachment } from "./doc-attachment";

const doc = (text: string) => docAttachment({ path: "doc/product/demo/index.md", text, hash: "h" });

it("a short document is quoted whole", () => {
  expect(doc("# Vision\n\nBuild it.\n").text).toBe("# Vision\n\nBuild it.\n");
});

it("a long document is cut at a line boundary and says how much is missing", () => {
  const line = "x".repeat(99);
  const text = Array.from({ length: 30 }, () => line).join("\n");
  const quote = doc(text).text;
  const lines = quote.split("\n");
  const shown = lines.slice(0, -1).join("\n");
  expect(lines.at(-1)).toBe(
    `[…${Array.from(text).length - shown.length} more characters; read the source]`,
  );
  expect(shown.length).toBeLessThanOrEqual(2_000);
  expect(lines.slice(0, -1).every((each) => each === line)).toBe(true);
});

it("a single line past the limit is cut on a character boundary, with the true count", () => {
  const quote = doc("😀".repeat(2_500)).text;
  expect(quote).toBe(`${"😀".repeat(2_000)}\n[…500 more characters; read the source]`);
});

it("the formatter leaves the marked quote alone, so nothing is cut twice", () => {
  const quote = doc("y".repeat(5_000)).text;
  expect(quoteLines(quote)).toEqual(quote.split("\n"));
});
