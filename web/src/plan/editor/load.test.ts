import * as ts from "typescript";
import { describe, expect, it, vi } from "vitest";

import { loadCodeMirror } from "./load";

vi.mock("./codemirror", () => ({ createSourceEditor: vi.fn() }));

const PLAN_PREFIX = "/src/plan/";

// Every plan source except the lazy module itself and tests, keyed relative to src/plan.
const planSources = Object.entries(
  import.meta.glob(["/src/plan/**/*.{ts,tsx}", "!**/*.test.ts", "!**/*.test.tsx"], {
    query: "?raw",
    import: "default",
    eager: true,
  }) as Record<string, string>,
)
  .filter(([key]) => !key.endsWith("/codemirror.ts"))
  .map(([key, source]) => [key.slice(PLAN_PREFIX.length), source] as const);

function staticCodeMirrorImports(path: string, source: string, scope = "@codemirror/"): string[] {
  const file = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true);
  const violations: string[] = [];
  function visit(node: ts.Node) {
    let specifier: ts.Expression | undefined;
    if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) {
      specifier = node.moduleSpecifier;
    } else if (
      ts.isImportEqualsDeclaration(node) &&
      ts.isExternalModuleReference(node.moduleReference)
    ) {
      specifier = node.moduleReference.expression;
    }
    if (specifier && ts.isStringLiteral(specifier) && specifier.text.startsWith(scope)) {
      const line = file.getLineAndCharacterOfPosition(node.getStart(file)).line + 1;
      violations.push(`${path}:${line}: ${specifier.text}`);
    }
    ts.forEachChild(node, visit);
  }
  visit(file);
  return violations;
}

describe("CodeMirror lazy boundary", () => {
  it("memoizes the dynamic import", async () => {
    const first = loadCodeMirror();
    expect(loadCodeMirror()).toBe(first);
    expect((await first).createSourceEditor).toBeTypeOf("function");
  });

  it("recognizes static imports and re-exports without matching comments or dynamic imports", () => {
    const source = `
      // import "@codemirror/comment";
      import "@codemirror/view";
      export { EditorState } from "@codemirror/state";
      import state = require("@codemirror/state");
      import("@codemirror/search");
    `;
    expect(staticCodeMirrorImports("example.ts", source)).toHaveLength(3);
  });

  it("keeps static CodeMirror imports in the lazy module", () => {
    const violations = planSources.flatMap(([path, source]) =>
      staticCodeMirrorImports(path, source),
    );
    expect(violations).toEqual([]);
  });
});

// @lezer/* belongs to the lazy chunk as much as @codemirror/*: the rule covers every source
// under src, not only src/plan.
const appSources = Object.entries(
  import.meta.glob(["/src/**/*.{ts,tsx}", "!**/*.test.ts", "!**/*.test.tsx"], {
    query: "?raw",
    import: "default",
    eager: true,
  }) as Record<string, string>,
).filter(([key]) => key !== "/src/plan/editor/codemirror.ts");

describe("Lezer lazy boundary", () => {
  it("recognizes static @lezer/ imports and re-exports without matching dynamic imports", () => {
    const source = `
      // import "@lezer/comment";
      import { tags } from "@lezer/highlight";
      export { parser } from "@lezer/markdown";
      import("@lezer/common");
      import "@lezer-like/other";
    `;
    expect(staticCodeMirrorImports("example.ts", source, "@lezer/")).toHaveLength(2);
  });

  it("scans the whole app, not only the plan code", () => {
    expect(appSources.length).toBeGreaterThan(planSources.length);
    expect(appSources.some(([key]) => key.startsWith("/src/components/"))).toBe(true);
  });

  it("keeps static @lezer/ imports in the lazy module", () => {
    const violations = appSources.flatMap(([path, source]) =>
      staticCodeMirrorImports(path, source, "@lezer/"),
    );
    expect(violations).toEqual([]);
  });
});
