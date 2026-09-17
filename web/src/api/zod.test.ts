import { afterEach, describe, expect, it } from "vitest";

// The dashboard's CSP has no `script-src`, so zod's `new Function` eval probe
// (its JIT fast-path check) logs a `securitypolicyviolation` even though the
// throw it triggers is caught. `src/api/zod.ts` calls `z.config({ jitless:
// true })` before any schema is built to skip that probe entirely.
//
// Static imports run before this file's body, so `@/api/schema` (and the
// `zod` it pulls in) must NOT be imported at the top of this file: that
// would build the schemas, and thus run the probe, before the `Function`
// proxy below is installed. Everything that touches zod is loaded via a
// dynamic `import()` inside the test instead.

describe("zod jitless setup", () => {
  const originalFunction = globalThis.Function;

  afterEach(() => {
    globalThis.Function = originalFunction;
  });

  it("builds and parses the snapshot schema without calling Function", async () => {
    let evalCalls = 0;
    const proxiedFunction = new Proxy(originalFunction, {
      construct(target, args, newTarget) {
        evalCalls += 1;
        return Reflect.construct(target, args, newTarget);
      },
      apply(target, thisArg, args) {
        evalCalls += 1;
        return Reflect.apply(target, thisArg, args);
      },
    });
    globalThis.Function = proxiedFunction as unknown as FunctionConstructor;

    try {
      const [{ snapshotSchema }, { default: fixture }] = await Promise.all([
        import("@/api/schema"),
        import("@/api/fixtures/snapshot.json"),
      ]);

      snapshotSchema.parse(fixture);

      expect(evalCalls).toBe(0);
    } finally {
      globalThis.Function = originalFunction;
    }
  });
});
