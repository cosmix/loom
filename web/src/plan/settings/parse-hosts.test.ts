import { describe, expect, it } from "vitest";

import { countHosts, hostsToAdd, isList, normaliseHost, parseHosts } from "./parse-hosts";

const hosts = (text: string, existing: string[] = []) =>
  parseHosts(text, existing).map((entry) => `${entry.status}:${entry.host}`);

describe("separators", () => {
  it("splits on commas", () => {
    expect(hosts("docs.rs,bun.com, nodejs.org")).toEqual([
      "add:docs.rs",
      "add:bun.com",
      "add:nodejs.org",
    ]);
  });

  it("splits on new lines", () => {
    expect(hosts("docs.rs\nbun.com\r\nnodejs.org\n")).toEqual([
      "add:docs.rs",
      "add:bun.com",
      "add:nodejs.org",
    ]);
  });

  it("splits a mix of commas, new lines and spaces, and drops empty pieces", () => {
    expect(hosts("docs.rs, bun.com\n\n  nodejs.org ,, \t react.dev;vite.dev")).toEqual([
      "add:docs.rs",
      "add:bun.com",
      "add:nodejs.org",
      "add:react.dev",
      "add:vite.dev",
    ]);
  });

  it("reads empty and blank text as no entries", () => {
    expect(parseHosts("", [])).toEqual([]);
    expect(parseHosts(" \n , ;\t", [])).toEqual([]);
  });
});

describe("normalisation", () => {
  it.each([
    ["https://docs.rs/tokio/latest?x=1#top", "docs.rs"],
    ["HTTP://Docs.RS", "docs.rs"],
    ["user:pw@docs.rs/x", "docs.rs"],
    ["docs.rs:443", "docs.rs"],
    ["https://docs.rs:8443/a", "docs.rs"],
    ["*.docs.rs", "docs.rs"],
    ["docs.rs.", "docs.rs"],
    ["docs.rs/", "docs.rs"],
    ["docs.rs?q=1", "docs.rs"],
  ])("reduces %s to %s", (piece, host) => {
    expect(normaliseHost(piece)).toBe(host);
    expect(hosts(piece)).toEqual([`add:${host}`]);
  });

  it("keeps the piece as pasted in `raw`", () => {
    expect(parseHosts("https://Docs.rs/x", [])[0]).toMatchObject({
      raw: "https://Docs.rs/x",
      host: "docs.rs",
    });
  });
});

describe("classification", () => {
  it("marks a repeat in the paste after its first appearance, whatever its spelling", () => {
    expect(hosts("docs.rs https://DOCS.rs/a *.docs.rs")).toEqual([
      "add:docs.rs",
      "repeated:docs.rs",
      "repeated:docs.rs",
    ]);
  });

  it("marks a host already on the list, whatever its spelling", () => {
    expect(hosts("docs.rs, new.dev, https://GitHub.com/x", ["docs.rs", "github.com"])).toEqual([
      "listed:docs.rs",
      "add:new.dev",
      "listed:github.com",
    ]);
  });

  it("calls a host already on the list listed even when it repeats in the paste", () => {
    expect(hosts("docs.rs docs.rs", ["docs.rs"])).toEqual(["listed:docs.rs", "listed:docs.rs"]);
  });

  it.each([
    ["localhost:3000", "Names this machine"],
    ["app.localhost", "Names this machine"],
    ["intranet", "Needs two labels, as in docs.rs"],
    ["192.168.1.20", "An IP address; list a host name"],
    ["[::1]", "An IP address; list a host name"],
    ["1.2.3", "An IP address; list a host name"],
    ["bad_host.com", "Not a bare host name"],
    ["a$b.com", "Not a bare host name"],
    ["docs..rs", "Not a valid host name"],
    [`${"a".repeat(64)}.com`, "Not a valid host name"],
    [`${"a.".repeat(130)}com`, "Not a valid host name"],
    ["https://", "Not a host name"],
    ["*.", "Not a host name"],
  ])("refuses %s with its reason", (piece, reason) => {
    const [entry] = parseHosts(piece, []);
    expect(entry).toMatchObject({ status: "invalid", reason, raw: piece, host: piece });
  });

  it("lets an IDN name through for the server to punycode", () => {
    expect(hosts("παράδειγμα.δοκιμή")).toEqual(["add:παράδειγμα.δοκιμή"]);
  });

  it("never lists an invalid piece as repeated or listed", () => {
    expect(hosts("localhost localhost", ["localhost"])).toEqual([
      "invalid:localhost",
      "invalid:localhost",
    ]);
  });
});

describe("counts and the hosts to add", () => {
  const entries = parseHosts(
    "docs.rs, bun.com, bun.com, github.com, localhost, https://nodejs.org/en",
    ["github.com"],
  );

  it("counts each status", () => {
    expect(countHosts(entries)).toEqual({ add: 3, listed: 1, repeated: 1, invalid: 1 });
  });

  it("lists the hosts to add in paste order, each once", () => {
    expect(hostsToAdd(entries)).toEqual(["docs.rs", "bun.com", "nodejs.org"]);
  });
});

describe("isList", () => {
  it.each([
    ["docs.rs", false],
    ["  docs.rs  ", false],
    ["", false],
    ["docs.rs bun.com", true],
    ["docs.rs,bun.com", true],
    ["docs.rs\n", false],
    ["docs.rs\nbun.com", true],
  ])("%j is %s", (text, expected) => {
    expect(isList(text)).toBe(expected);
  });
});
