import { expect, it } from "vitest";

import { lineCounts } from "./line-diff";

it("identical texts have no changes", () => {
  expect(lineCounts("a\nb\nc", "a\nb\nc")).toEqual({ added: 0, removed: 0 });
});

it("counts appended and deleted lines", () => {
  expect(lineCounts("a\nb", "a\nb\nc\nd")).toEqual({ added: 2, removed: 0 });
  expect(lineCounts("a\nb\nc\nd", "a\nd")).toEqual({ added: 0, removed: 2 });
});

it("a changed line is one added and one removed", () => {
  expect(lineCounts("a\nb\nc", "a\nB\nc")).toEqual({ added: 1, removed: 1 });
});

it("moved lines count on both sides, only where they differ", () => {
  expect(lineCounts("a\nb\nc\nd", "d\na\nb\nc")).toEqual({ added: 1, removed: 1 });
});

it("handles interleaved edits and empty sides", () => {
  expect(lineCounts("a\nb\nc\nd\ne", "a\nx\nc\ny\ne\nz")).toEqual({ added: 3, removed: 2 });
  expect(lineCounts("", "a\nb")).toEqual({ added: 2, removed: 0 });
  expect(lineCounts("a\nb", "")).toEqual({ added: 0, removed: 2 });
  expect(lineCounts("", "")).toEqual({ added: 0, removed: 0 });
});

it("a final newline ends the last line instead of adding one", () => {
  expect(lineCounts("", "a\nb\n")).toEqual({ added: 2, removed: 0 });
  expect(lineCounts("old\nkeep\n", "new\nkeep\n")).toEqual({ added: 1, removed: 1 });
});

it("approximates a rewrite beyond the edit-distance cap without hanging", () => {
  const before = Array.from({ length: 30000 }, (_, index) => `old ${index}`).join("\n");
  const after = Array.from({ length: 30000 }, (_, index) => `new ${index}`).join("\n");
  const start = performance.now();
  expect(lineCounts(before, after)).toEqual({ added: 30000, removed: 30000 });
  expect(performance.now() - start).toBeLessThan(2000);
});

it("keeps the shared prefix and suffix out of an approximate count", () => {
  const middle = (tag: string) => Array.from({ length: 3000 }, (_, index) => `${tag} ${index}`);
  const before = ["head", ...middle("old"), "tail"].join("\n");
  const after = ["head", ...middle("new"), "tail"].join("\n");
  expect(lineCounts(before, after)).toEqual({ added: 3000, removed: 3000 });
});

it("stays fast on a large document with a few edits", () => {
  const lines = Array.from({ length: 20000 }, (_, index) => `line ${index}`);
  const edited = [...lines];
  edited[100] = "changed";
  edited.splice(15000, 1);
  expect(lineCounts(lines.join("\n"), edited.join("\n"))).toEqual({ added: 1, removed: 2 });
});
