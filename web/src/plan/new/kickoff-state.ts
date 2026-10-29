import { atom } from "jotai/vanilla";

import type { KickoffView } from "@/plan/api/schema-kickoff";

/** The kickoff started by this browser tab; deliberately never persisted. */
export const kickoffSessionAtom = atom<string | null>(null);

export function isKickoffSession(
  sessionId: string | null,
  tab: string | null,
  view: KickoffView | null,
): boolean {
  return sessionId != null && (sessionId === tab || view?.session === sessionId);
}
