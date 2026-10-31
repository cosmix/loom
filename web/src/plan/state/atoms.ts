import { atom } from "jotai/vanilla";

import type {
  AuthoringView,
  ChecklistView,
  OfferView,
  PresenceList,
  ProjectChecklistView,
  ServerInfo,
  StandalonePlanView,
  You,
} from "@/plan/api/schema";
import type { DraftTeamView, KickoffView } from "@/plan/api/schema-kickoff";
import type { connectPlanSocket } from "@/plan/api/socket";

export type ConnectionPhase = "connecting" | "open" | "reconnecting" | "unauthorized";
export interface ConnectionState {
  phase: ConnectionPhase;
  lastMessageAt: number | null;
  everOpen: boolean;
}

export const serverInfoAtom = atom<ServerInfo | null>(null);
export const youAtom = atom<You | null>(null);
export const projectAtom = atom<ProjectChecklistView | null>(null);
// Keep the last feature screen selected across project updates.
export const activeFeatureAtom = atom<string | null>(null);
export const featuresAtom = atom<ChecklistView[]>((get) => get(projectAtom)?.checklists ?? []);
export const standaloneAtom = atom<StandalonePlanView[]>(
  (get) => get(projectAtom)?.standalone ?? [],
);
export const movesAtom = atom<OfferView[]>((get) => get(projectAtom)?.moves ?? []);
export const kickoffAtom = atom<KickoffView | null>(null);
export const draftTeamAtom = atom<DraftTeamView | null>(null);

// Preserve the single-checklist seam for existing consumers and fixtures.
export const checklistAtom = atom(
  (get) => {
    const checklists = get(featuresAtom);
    const activeFeature = get(activeFeatureAtom);
    return (
      checklists.find((checklist) => checklist.feature === activeFeature) ??
      (checklists.length === 1 ? checklists[0] : null)
    );
  },
  (_get, set, value: ChecklistView | null) => {
    set(projectAtom, { checklists: value ? [value] : [], standalone: [], moves: [] });
    if (value !== null) set(activeFeatureAtom, value.feature);
  },
);
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
  // The banner carries the message and the fix; a blocked control carries this one word.
  if (get(serverInfoAtom)?.read_only.length) return "Read-only";
  if (get(stoppingAtom)) return "loom plan is stopping";
  const holder = get(authoringAtom)?.holder;
  return holder ? `${holder.name} holds authoring` : null;
});
