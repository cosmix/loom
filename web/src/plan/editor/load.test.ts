import * as ts from "typescript";
import { describe, expect, it, vi } from "vitest";

import { loadCodeMirror } from "./load";

vi.mock("./codemirror", () => ({ createSourceEditor: vi.fn() }));

const PLAN_PREFIX = "/src/plan/";

// The lazy chunk: the only modules that may import @codemirror/* or @lezer/* statically.
// codemirror.ts imports the other three; line-break.ts is pure and imports no CodeMirror.
const CHUNK_MODULES = ["codemirror", "live-preview", "md-commands", "marks-gutter"].map(
  (name) => `/src/plan/editor/${name}.ts`,
);

// Every plan source except the chunk modules and tests, keyed relative to src/plan.
const allPlanSources = import.meta.glob(
  ["/src/plan/**/*.{ts,tsx}", "!**/*.test.ts", "!**/*.test.tsx"],
  { query: "?raw", import: "default", eager: true },
) as Record<string, string>;
const planSources = Object.entries(allPlanSources)
  .filter(([key]) => !CHUNK_MODULES.includes(key))
  .map(([key, source]) => [key.slice(PLAN_PREFIX.length), source] as const);

// Every static import or export-from specifier (type-only ones included) with its line, plus
// dynamic import("...") calls when asked.
function moduleSpecifiers(path: string, source: string, dynamic = true) {
  const file = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true);
  const found: { text: string; line: number }[] = [];
  function visit(node: ts.Node) {
    let specifier: ts.Expression | undefined;
    if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) {
      specifier = node.moduleSpecifier;
    } else if (
      ts.isImportEqualsDeclaration(node) &&
      ts.isExternalModuleReference(node.moduleReference)
    ) {
      specifier = node.moduleReference.expression;
    } else if (
      dynamic &&
      ts.isCallExpression(node) &&
      node.expression.kind === ts.SyntaxKind.ImportKeyword
    ) {
      specifier = node.arguments[0];
    }
    if (specifier && ts.isStringLiteral(specifier)) {
      const line = file.getLineAndCharacterOfPosition(node.getStart(file)).line + 1;
      found.push({ text: specifier.text, line });
    }
    ts.forEachChild(node, visit);
  }
  visit(file);
  return found;
}

function staticCodeMirrorImports(path: string, source: string, scope = "@codemirror/"): string[] {
  return moduleSpecifiers(path, source, false)
    .filter((specifier) => specifier.text.startsWith(scope))
    .map((specifier) => `${path}:${specifier.line}: ${specifier.text}`);
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

  it("keeps static CodeMirror imports in the lazy chunk", () => {
    const violations = planSources.flatMap(([path, source]) =>
      staticCodeMirrorImports(path, source),
    );
    expect(violations).toEqual([]);
  });

  it("sees the static CodeMirror imports of every chunk module", () => {
    for (const module of CHUNK_MODULES) {
      expect(allPlanSources[module], module).toBeDefined();
      expect(staticCodeMirrorImports(module, allPlanSources[module] ?? ""), module).not.toEqual([]);
    }
  });

  it("keeps the chunk helpers behind codemirror.ts", () => {
    const helper = /(?:^|\/)(?:live-preview|md-commands|marks-gutter)$/;
    const violations = planSources.flatMap(([path, source]) =>
      moduleSpecifiers(path, source)
        .filter((specifier) => helper.test(specifier.text))
        .map((specifier) => `${path}:${specifier.line}: ${specifier.text}`),
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
).filter(([key]) => !CHUNK_MODULES.includes(key));

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

  it("keeps static @lezer/ imports in the lazy chunk", () => {
    const violations = appSources.flatMap(([path, source]) =>
      staticCodeMirrorImports(path, source, "@lezer/"),
    );
    expect(violations).toEqual([]);
  });
});
