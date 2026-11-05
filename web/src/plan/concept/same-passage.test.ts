import { expect, it } from "vitest";

import { firstPassageKeys } from "./same-passage";
import type { ColumnTarget } from "./types";

const passage = (id: string, runOf: string, parent: string | null = null): ColumnTarget => ({
  kind: "passage",
  id,
  level: 2,
  runOf,
  parent,
});

it("keeps the first showing of a passage and ignores sentence targets", () => {
  const order: ColumnTarget[] = [
    { kind: "sentence", id: "doc/a.md#stages", level: 0, runOf: null },
    passage("doc/a.md#stages", "a"),
    passage("doc/a.md#intent", "b"),
    passage("doc/a.md#stages", "c"),
  ];

  const first = firstPassageKeys(order);

  expect([...first]).toEqual([
    ["doc/a.md#stages", "p||a|doc/a.md#stages"],
    ["doc/a.md#intent", "p||b|doc/a.md#intent"],
  ]);
});
