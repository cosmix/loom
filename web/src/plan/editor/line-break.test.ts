import { describe, expect, it } from "vitest";

import { detectLineBreak, withLineBreak } from "./line-break";

describe("detectLineBreak", () => {
  it("returns null when the text has no carriage return", () => {
    expect(detectLineBreak("")).toBeNull();
    expect(detectLineBreak("one")).toBeNull();
    expect(detectLineBreak("one\ntwo\n")).toBeNull();
  });

  it("returns CRLF when every break is CRLF", () => {
    expect(detectLineBreak("a\r\nb\r\n")).toBe("\r\n");
    expect(detectLineBreak("a\r\nb")).toBe("\r\n");
  });

  it("returns CR when every break is a lone CR", () => {
    expect(detectLineBreak("a\rb\r")).toBe("\r");
    expect(detectLineBreak("a\r")).toBe("\r");
  });

  it("returns LF for a mix, so each stray CR stays a character of its line", () => {
    expect(detectLineBreak("a\r\nb\nc\r")).toBe("\n");
    expect(detectLineBreak("a\r\nb\rc")).toBe("\n");
    expect(detectLineBreak("a\rb\nc")).toBe("\n");
  });

  it("treats a trailing lone CR beside CRLF as a mix", () => {
    expect(detectLineBreak("a\r\nb\r")).toBe("\n");
  });
});

describe("withLineBreak", () => {
  it("rewrites every kind of break to the given one", () => {
    expect(withLineBreak("a\nb\r\nc\rd", "\r\n")).toBe("a\r\nb\r\nc\r\nd");
    expect(withLineBreak("a\r\nb", "\n")).toBe("a\nb");
    expect(withLineBreak("no breaks", "\r")).toBe("no breaks");
  });
});
