import { afterEach, describe, expect, it, vi } from "vitest";

import { fetchTerminalHistory } from "@/api/terminal-history";

afterEach(() => vi.unstubAllGlobals());

function stubFetch(response: Response) {
  const fetchMock = vi.fn(async (_url: string, _init?: RequestInit) => response);
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

describe("fetchTerminalHistory", () => {
  it("returns the saved bytes untouched", async () => {
    const bytes = new Uint8Array([27, 91, 51, 49, 109, 114, 10]);
    const fetchMock = stubFetch(new Response(bytes, { status: 200 }));

    const history = await fetchTerminalHistory("session-a");

    expect(history).toEqual(bytes);
    expect(fetchMock).toHaveBeenCalledWith("/api/terminal-history/session-a", {
      credentials: "same-origin",
    });
  });

  it("encodes the session id into the path", async () => {
    const fetchMock = stubFetch(new Response(null, { status: 404 }));

    await fetchTerminalHistory("a/b c");

    expect(fetchMock.mock.calls[0]?.[0]).toBe("/api/terminal-history/a%2Fb%20c");
  });

  it("returns null for a session with no history", async () => {
    stubFetch(new Response("no terminal history for that session", { status: 404 }));

    expect(await fetchTerminalHistory("session-a")).toBeNull();
  });

  it.each([401, 500])("rejects on status %i", async (status) => {
    stubFetch(new Response("refused", { status }));

    await expect(fetchTerminalHistory("session-a")).rejects.toThrow(String(status));
  });
});
