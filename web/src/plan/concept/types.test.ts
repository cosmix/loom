import { expectTypeOf, it } from "vitest";

import type { FocusTarget, FoldNote, OpenRun, SourceAnchorRef } from "./types";

it("pins the open run shape", () => {
  expectTypeOf<OpenRun>().toEqualTypeOf<{
    level: 0 | 1 | 2;
    sentenceId: string;
    restsOn: SourceAnchorRef[];
    parent: string | null;
  }>();
});

it("pins the source anchor reference shape", () => {
  expectTypeOf<{ [K in keyof SourceAnchorRef]: SourceAnchorRef[K] }>().toEqualTypeOf<{
    path: string;
    anchor: string;
    title?: string;
  }>();
});

it("pins the fold note shape", () => {
  expectTypeOf<FoldNote>().toEqualTypeOf<{
    paragraphOf: string;
    text: string;
    path: string;
  }>();
});

it("pins the focus target shape", () => {
  expectTypeOf<FocusTarget>().toEqualTypeOf<{
    kind: "sentence" | "passage";
    id: string;
    level: 0 | 1 | 2;
    runOf: string | null;
  }>();
});
