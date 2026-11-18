import { z } from "@/api/zod";
import { apiGet, apiPost } from "@/plan/api/client";

/** What the server answers a session's WebFetch request with before anyone is asked. */
export const fetchModeSchema = z.enum(["ask", "listed", "always", "never"]);
/**
 * The project's fetch policy; the server validates and normalises the domains. `revision` is
 * the server's count of changes: a save names the revision it was built on, and the server
 * refuses (409) one built on an older read.
 */
export const fetchPolicySchema = z.strictObject({
  mode: fetchModeSchema,
  domains: z.array(z.string()),
  revision: z.number().int().nonnegative(),
});

export type FetchMode = z.infer<typeof fetchModeSchema>;
export type FetchPolicy = z.infer<typeof fetchPolicySchema>;

const PATH = "/api/web-fetch";

export function getFetchPolicy(signal?: AbortSignal) {
  return apiGet(PATH, fetchPolicySchema, signal);
}

/** The whole policy at once; only the authoring holder may save it. */
export function saveFetchPolicy(policy: FetchPolicy) {
  return apiPost(PATH, policy, fetchPolicySchema, { contentType: "json" });
}
