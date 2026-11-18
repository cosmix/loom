import { describe, expect, it } from "vitest";

import { fenceBody, withoutFence, withoutKickoffFence } from "./kickoff-fence";

const tick = "`".repeat(3);
const block = `${tick}loom-kickoff\n{"problem":"p","questions":["Which hosts?"]}\n${tick}`;

describe("withoutKickoffFence", () => {
  it("drops the block and keeps the prose before and after it", () => {
    const text = `I read these first.\n\n${block}\n\nThat is all.`;

    expect(withoutKickoffFence(text)).toBe("I read these first.\n\n\nThat is all.");
  });

  it("returns text without a kickoff block unchanged", () => {
    const text = `Use this:\n\n${tick}json\n{"a":1}\n${tick}\n`;

    expect(withoutKickoffFence(text)).toBe(text);
  });

  it("drops a block that is still streaming, with no closing fence", () => {
    const text = `Working on it.\n\n${tick}loom-kickoff\n{"problem":"p","ques`;

    expect(withoutKickoffFence(text)).toBe("Working on it.");
  });

  it("leaves a block that is only a kickoff block as an empty string", () => {
    expect(withoutKickoffFence(`${block}\n`)).toBe("");
  });

  it("drops a tilde fence and a longer backtick fence the same way", () => {
    expect(withoutKickoffFence("Hi\n~~~loom-kickoff\n{}\n~~~\nBye")).toBe("Hi\nBye");
    const long = "`".repeat(4);
    expect(
      withoutKickoffFence(`Hi\n${long}loom-kickoff\n${tick}\nstill inside\n${long}\nBye`),
    ).toBe("Hi\nBye");
  });

  it("matches the info string exactly: a longer name stays", () => {
    const other = `${tick}loom-kickoff-notes\nkept\n${tick}`;

    expect(withoutKickoffFence(other)).toBe(other);
  });

  it("keeps a kickoff example that sits inside a longer fence", () => {
    const long = "`".repeat(4);
    const text = `${long}md\n${block}\n${long}`;

    expect(withoutKickoffFence(text)).toBe(text);
  });

  it("drops two kickoff blocks and keeps a code block between them", () => {
    const code = `${tick}ts\nconst a = 1;\n${tick}`;

    expect(withoutKickoffFence(`${block}\n${code}\n${block}`)).toBe(code);
  });
});

describe("withoutFence", () => {
  it("drops only the fence with the given info string", () => {
    const closing = `${tick}loom-closing\n{"decisions":[]}\n${tick}`;

    expect(withoutFence(`Done.\n\n${closing}`, "loom-closing")).toBe("Done.");
    expect(withoutFence(`${block}\n${closing}`, "loom-closing")).toBe(block);
  });
});

describe("fenceBody", () => {
  const closing = (body: string) => `${tick}loom-closing\n${body}\n${tick}`;

  it("returns the body of the last matching fence", () => {
    const text = `${closing('{"a":1}')}\nthen\n${closing('{"a":2}\n{"b":3}')}`;

    expect(fenceBody(text, "loom-closing")).toBe('{"a":2}\n{"b":3}');
  });

  it("returns null without a matching fence", () => {
    expect(fenceBody("plain text", "loom-closing")).toBeNull();
    expect(fenceBody(`${tick}json\n{}\n${tick}`, "loom-closing")).toBeNull();
    expect(fenceBody(`${tick}loom-closing-notes\n{}\n${tick}`, "loom-closing")).toBeNull();
  });

  it("returns null for a fence that is still open", () => {
    expect(fenceBody(`${tick}loom-closing\n{"decisions":[`, "loom-closing")).toBeNull();
  });

  it("ignores a block quoted inside a longer fence", () => {
    const long = "`".repeat(4);

    expect(fenceBody(`${long}md\n${closing("{}")}\n${long}`, "loom-closing")).toBeNull();
  });

  it("returns null when the last block is still open, even after a closed one", () => {
    const text = `${closing('{"a":1}')}\n${tick}loom-closing\n{"a":`;

    expect(fenceBody(text, "loom-closing")).toBeNull();
  });
});

describe("CRLF text", () => {
  const crlf = (text: string) => text.replaceAll("\n", "\r\n");

  it("reads a loom-done fence and hides it, as the server does", () => {
    const text = crlf(`Done.\n${tick}loom-done\n{"a":1}\n${tick}\n`);

    expect(fenceBody(text, "loom-done", true)).toBe('{"a":1}\r');
    expect(withoutKickoffFence(text)).toBe("Done.");
  });

  it("reads a loom-closing fence and drops a streaming kickoff fence", () => {
    expect(fenceBody(crlf(`${tick}loom-closing\n{"a":1}\n${tick}\n`), "loom-closing")).toBe(
      '{"a":1}\r',
    );
    expect(withoutKickoffFence(crlf(`Hi.\n${tick}loom-kickoff\n{"problem"`))).toBe("Hi.");
  });
});
