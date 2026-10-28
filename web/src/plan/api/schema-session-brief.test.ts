import { afterEach, describe, expect, it, vi } from "vitest";
import fixture from "@/plan/api/fixtures/session-brief.json";
import { getSessionBrief, sessionBriefSchema } from "@/plan/api/schema-session-brief";

afterEach(() => vi.unstubAllGlobals());

describe("session brief", () => {
  it("parses the session brief fixture", () => {
    expect(sessionBriefSchema.parse(fixture)).toEqual(fixture);
  });

  it("returns a 404 from apiGet for an unknown session", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        new Response('{"error":"unknown_session","message":"unknown session"}', { status: 404 }),
      );
    vi.stubGlobal("fetch", fetchMock);

    expect(await getSessionBrief("session & one")).toMatchObject({ ok: false, status: 404 });
    expect(fetchMock).toHaveBeenCalledWith("/api/session/brief?session=session%20%26%20one", {
      method: "GET",
      signal: undefined,
      credentials: "same-origin",
    });
  });
});
