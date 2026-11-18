import type { DraftTeamView, TeammateStatus } from "@/plan/api/schema-kickoff";
import { ADDRESS_FINDINGS_ID } from "@/plan/trace/findings-model";

/** The one vocabulary every surface uses for a document of a team run. */
export type TeamDocState = "Drafting" | "Queued" | "Drafted";

export interface TeamDocument {
  path: string;
  state: TeamDocState;
  /** The teammate that has written to the document; null while no teammate has touched it. */
  owner: string | null;
}

/**
 * The server lists a teammate's documents in the order it first wrote to them, so a document no
 * teammate lists is queued, and one before the teammate's current document is drafted.
 */
export function documentState(path: string, teammate: TeammateStatus | undefined): TeamDocState {
  if (!teammate) return "Queued";
  if (teammate.state === "working" && teammate.current === path) return "Drafting";
  if (teammate.state === "idle" && teammate.current === null) return "Drafted";
  const currentIndex =
    teammate.current === null ? -1 : teammate.documents.indexOf(teammate.current);
  return teammate.documents.indexOf(path) < currentIndex ? "Drafted" : "Queued";
}

/** One document of the run, or null when the run does not cover `path`. */
export function teamDocument(team: DraftTeamView, path: string): TeamDocument | null {
  const teammate = team.teammates.find((candidate) => candidate.documents.includes(path));
  if (!teammate && !team.documents.includes(path)) return null;
  return {
    path,
    state: documentState(path, teammate),
    owner: teammate?.name ?? null,
  };
}

/**
 * Every queued document drafted: the lead session lives on, but the team has nothing to stop. The
 * server leaves an address run's ledger out of `documents`: only the lead writes it, so no
 * teammate row would ever read it drafted. A run with no teammates reports no team at all.
 */
export function teamFinished(team: DraftTeamView): boolean {
  return (
    team.documents.length > 0 &&
    team.documents.every((path) => teamDocument(team, path)?.state === "Drafted")
  );
}

/** What a team panel calls itself, its run and a document's state; `TeamDocState` stays the model. */
export interface TeamWords {
  /** The panel's accessible name. */
  region: string;
  running: string;
  finished: string;
  state: Record<TeamDocState, string>;
}

const DRAFT_WORDS: TeamWords = {
  region: "Draft team",
  running: "Drafting with a team",
  finished: "Drafted with a team",
  state: { Drafting: "Drafting", Queued: "Queued", Drafted: "Drafted" },
};

const ADDRESS_WORDS: TeamWords = {
  region: "Findings team",
  running: "Addressing findings with a team",
  finished: "Addressed with a team",
  state: { Drafting: "Editing", Queued: "Queued", Drafted: "Edited" },
};

/**
 * The words for the team of the session started by `offer`: an address-findings run edits
 * documents that exist, every other run drafts them.
 */
export function teamWords(offer: string | null | undefined): TeamWords {
  return offer === ADDRESS_FINDINGS_ID ? ADDRESS_WORDS : DRAFT_WORDS;
}
