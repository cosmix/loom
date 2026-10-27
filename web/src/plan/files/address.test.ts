import { describe, expect, it } from "vitest";
import { docRel, parseAddress, repoRel, resolveLink, toAddress } from "./address";

describe("file addresses", () => {
  it("round trips paths, anchors, and diff queries", () => {
    const path = "doc/plans/a space/100%.md";
    const url = new URL(`${toAddress(path, "a heading")}?ignored=1`, "https://example.com");
    // Queries precede the fragment in a navigable address.
    url.search = `?diff=turn&turn=${encodeURIComponent("3f9a/t2")}`;
    url.hash = encodeURIComponent("a heading");
    expect(parseAddress(url)).toEqual({ path, anchor: "a heading", diff: "turn", turn: "3f9a/t2" });
    expect(docRel(repoRel("plans/x.md"))).toBe("plans/x.md");
    expect(repoRel(docRel(path))).toBe(path);
    expect(toAddress("plans/x.md")).toBe("/files/plans/x.md");
  });

  it("resolves document and anchor links within doc", () => {
    expect(resolveLink("doc/plans/x.md", "other%20file.md#hello%20world")).toEqual({
      kind: "doc",
      path: "doc/plans/other file.md",
      anchor: "hello world",
    });
    expect(resolveLink("plans/x.md", " #anchor ")).toEqual({ kind: "anchor", anchor: "anchor" });
    expect(resolveLink("plans/x.md", "../../outside.md")).toEqual({
      kind: "outside",
      target: "/outside.md",
    });
    expect(resolveLink("plans/x.md", "..%2F..%2Foutside.md").kind).toBe("outside");
  });

  it("classifies parsed external URLs and blocks unsafe schemes", () => {
    expect(resolveLink("plans/x.md", "//evil.example")).toEqual({
      kind: "external",
      href: "https://evil.example/",
    });
    expect(resolveLink("plans/x.md", "https://x")).toEqual({
      kind: "external",
      href: "https://x/",
    });
    expect(resolveLink("plans/x.md", "mailto:a@example.com").kind).toBe("external");
    for (const href of [
      "JaVaScRiPt:alert(1)",
      "java\nscript:alert(1)",
      "java\tscript:alert(1)",
      " javascript:alert(1)",
      "\u0000javascript:alert(1)",
      "data:text/html,x",
      "file:///tmp/x",
      "vbscript:x",
    ]) {
      expect(resolveLink("plans/x.md", href)).toEqual({ kind: "text" });
    }
  });
});
