import { atom } from "jotai/vanilla";

import type {
  AuthoringView,
  ChecklistView,
  PresenceList,
  ServerInfo,
  You,
} from "@/plan/api/schema";
import type { connectPlanSocket } from "@/plan/api/socket";

export type ConnectionPhase = "connecting" | "open" | "reconnecting" | "unauthorized";
export interface ConnectionState {
  phase: ConnectionPhase;
  lastMessageAt: number | null;
  everOpen: boolean;
}

export const serverInfoAtom = atom<ServerInfo | null>(null);
export const youAtom = atom<You | null>(null);
export const checklistAtom = atom<ChecklistView | null>(null);
export const presenceAtom = atom<PresenceList | null>(null);
export const authoringAtom = atom<AuthoringView | null>(null);
export const connectionAtom = atom<ConnectionState>({
  phase: "connecting",
  lastMessageAt: null,
  everOpen: false,
});
export const unauthorizedAtom = atom(false);
export const stoppingAtom = atom(false);
export const connectionEpochAtom = atom(0);
export const planSocketAtom = atom<ReturnType<typeof connectPlanSocket> | null>(null);

export const isHolderAtom = atom((get) => {
  const holder = get(authoringAtom)?.holder;
  const you = get(youAtom);
  return holder != null && you != null && holder.public_id === you.public_id;
});

export const canAuthorAtom = atom((get) => {
  if (get(isHolderAtom)) return true;
  const authoring = get(authoringAtom);
  const server = get(serverInfoAtom);
  return (
    authoring != null &&
    authoring.holder == null &&
    server != null &&
    server.read_only.length === 0 &&
    !get(stoppingAtom)
  );
});

export const authorBlockReasonAtom = atom<string | null>((get) => {
  if (get(canAuthorAtom)) return null;
  const problem = get(serverInfoAtom)?.read_only[0];
  if (problem) return `loom plan is in read-only mode: ${problem.message}`;
  if (get(stoppingAtom)) return "loom plan is stopping";
  const holder = get(authoringAtom)?.holder;
  return holder ? `${holder.name} holds authoring` : null;
});
