import { describe, expect, it } from "vitest";

import { withoutKickoffFence } from "./kickoff-fence";

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
