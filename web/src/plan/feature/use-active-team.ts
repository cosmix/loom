import { useAtomValue } from "jotai";

import type { DraftTeamView } from "@/plan/api/schema-kickoff";
import { sessionAtom } from "@/plan/chat/model";
import { isActiveSession } from "@/plan/chat/session-home";
import { allOffers } from "@/plan/feature/checklist-offers";
import { teamFinished } from "@/plan/feature/team-model";
import { draftTeamAtom, featuresAtom } from "@/plan/state/atoms";

export interface ActiveTeam {
  team: DraftTeamView;
  /** Every document is drafted; the lead session lives on but nothing is being written. */
  finished: boolean;
}

/**
 * The draft team of the chat session now open, when it works on `feature`. `draftTeamAtom` is set
 * by the server and never cleared for another session, so each consumer scopes it here: the team
 * panel, the section queue, the pace choice and the settle action all read one answer.
 */
export function useActiveTeam(feature: string): ActiveTeam | null {
  const team = useAtomValue(draftTeamAtom);
  const session = useAtomValue(sessionAtom);
  const features = useAtomValue(featuresAtom);
  if (team === null || !isActiveSession(session) || team.session !== session.id) return null;
  const offeredBy = features.filter((checklist) =>
    allOffers(checklist).some((offer) => session.offer != null && offer.id === session.offer),
  );
  // Offer IDs can be shared across features; an ambiguous offer cannot identify the owner.
  const inFeature =
    team.documents.length > 0
      ? team.documents.some(
          (path) =>
            path.startsWith(`doc/product/${feature}/`) || path.startsWith(`doc/plans/${feature}/`),
        )
      : offeredBy.length === 1 && offeredBy[0]?.feature === feature;
  return inFeature ? { team, finished: teamFinished(team) } : null;
}
