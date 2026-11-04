import { afterEach, describe, expect, it, vi } from "vitest";

import { MAX_UPLOAD_BYTES, uploadAttachment, uploadProblem, uploadType } from "./upload";

const stored = { id: "0123456789abcdef", name: "notes.md", media_type: "text/markdown", bytes: 5 };

function fileOf(name: string, type: string, content = "hello") {
  return new File([content], name, { type });
}

function sized(file: File, size: number): File {
  Object.defineProperty(file, "size", { value: size, configurable: true });
  return file;
}

function stubFetch(response: Response) {
  const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(response);
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("uploadType", () => {
  it.each([
    ["notes.md", "", "text/markdown"],
    ["NOTES.MD", "", "text/markdown"],
    ["notes.txt", "", "text/plain"],
    ["shot.png", "image/png", "image/png"],
    ["notes.txt", "text/plain; charset=utf-8", "text/plain"],
  ])("takes %s with type %j as %s", (name, type, expected) => {
    expect(uploadType({ name, type })).toBe(expected);
  });

  it.each([
    ["shot.png", ""],
    ["archive.zip", "application/zip"],
    ["script.js", "text/javascript"],
    ["page.svg", "image/svg+xml"],
  ])("refuses %s with type %j", (name, type) => {
    expect(uploadType({ name, type })).toBeNull();
  });
});

describe("uploadProblem", () => {
  it("names a file of a type outside the list", () => {
    expect(uploadProblem(fileOf("a.zip", "application/zip"))).toContain("a.zip");
  });

  it("refuses a file over 10 MiB and accepts one at the limit", () => {
    expect(
      uploadProblem(sized(fileOf("big.pdf", "application/pdf"), MAX_UPLOAD_BYTES + 1)),
    ).toContain("10 MiB");
    expect(uploadProblem(sized(fileOf("big.pdf", "application/pdf"), MAX_UPLOAD_BYTES))).toBeNull();
  });
});

describe("uploadAttachment", () => {
  it("posts the raw file with its type and its percent-encoded name", async () => {
    const fetchMock = stubFetch(new Response(JSON.stringify(stored)));
    const file = fileOf("my notes & more.md", "");

    await expect(uploadAttachment(file)).resolves.toEqual(stored);

    const [url, init] = fetchMock.mock.calls[0]!;
    expect(url).toBe("/api/attachments?name=my%20notes%20%26%20more.md");
    expect(init?.method).toBe("POST");
    expect(init?.body).toBe(file);
    expect(init?.headers).toEqual({ "Content-Type": "text/markdown" });
  });

  it("cuts a long name to 120 code points before encoding it", async () => {
    const fetchMock = stubFetch(new Response(JSON.stringify(stored)));
    const name = `${"é".repeat(60)}${"😀".repeat(80)}.md`;

    await uploadAttachment(fileOf(name, "text/markdown"));

    const sent = String(fetchMock.mock.calls[0]![0]).replace("/api/attachments?name=", "");
    expect(Array.from(decodeURIComponent(sent))).toHaveLength(120);
    expect(decodeURIComponent(sent)).toBe(Array.from(name).slice(0, 120).join(""));
  });

  it("refuses before sending a type outside the list or a file over the limit", async () => {
    const fetchMock = stubFetch(new Response("{}"));
    const big = sized(fileOf("big.png", "image/png"), MAX_UPLOAD_BYTES + 1);

    await expect(uploadAttachment(fileOf("a.zip", "application/zip"))).rejects.toThrow("a.zip");
    await expect(uploadAttachment(big)).rejects.toThrow("10 MiB");
    await expect(uploadAttachment(fileOf("shot.png", ""))).rejects.toThrow("shot.png");
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it.each([413, 415, 422, 507])("rejects with the server's reason for %i", async (status) => {
    stubFetch(
      new Response(JSON.stringify({ error: "refused", message: `reason ${status}` }), { status }),
    );

    await expect(uploadAttachment(fileOf("a.png", "image/png"))).rejects.toThrow(
      `reason ${status}`,
    );
  });

  it("falls back to the status when the refusal has no JSON body", async () => {
    stubFetch(new Response("gateway", { status: 502 }));

    await expect(uploadAttachment(fileOf("a.png", "image/png"))).rejects.toThrow("HTTP 502");
  });

  it("rejects a malformed answer and a network failure", async () => {
    stubFetch(new Response(JSON.stringify({ id: 7 })));
    await expect(uploadAttachment(fileOf("a.png", "image/png"))).rejects.toThrow(
      "Invalid server response",
    );

    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    await expect(uploadAttachment(fileOf("a.png", "image/png"))).rejects.toThrow("Failed to fetch");
  });
});
