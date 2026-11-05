import { Check, FileUp, FolderInput, History, Lightbulb, Plus, Wrench } from "lucide-react";

import { Untrusted } from "@/plan/chat/visible-controls";
import { followUpSeed } from "@/plan/feature/built-model";
import { DocumentStarter, type StarterDocument } from "@/plan/new/document-starter";
import type { FollowUpFeature } from "@/plan/new/follow-up-features";
import type { WorkKind } from "@/plan/new/kickoff-start";

export type Starter = {
  label: string;
  kind: WorkKind;
  /** The words the starter puts in the field; a follow-up leaves the field as it is. */
  text: string | null;
  Icon: typeof Lightbulb;
};

export const STARTERS: readonly Starter[] = [
  {
    label: "A new feature for this project",
    kind: "feature",
    text: "I want to build a new feature: ",
    Icon: Lightbulb,
  },
  {
    label: "A small fix or improvement",
    kind: "plan",
    text: "I want to fix or improve: ",
    Icon: Wrench,
  },
  {
    label: "Grow standalone plans into a feature",
    kind: "feature",
    text: "I have standalone plans that belong together and want to grow them into a feature.",
    Icon: FolderInput,
  },
  {
    label: "New plans for a feature that is already built",
    kind: "follow-up",
    text: null,
    Icon: History,
  },
];

/** Whether `seed` stands on a line of its own in `text`. */
export function hasSeed(text: string, seed: string): boolean {
  return text.split("\n").includes(seed);
}

/** `text` with `seed` on its own line at the end, or without the first line that is `seed`. */
export function toggleSeed(text: string, seed: string): string {
  if (hasSeed(text, seed)) {
    const lines = text.split("\n");
    lines.splice(lines.indexOf(seed), 1);
    return lines.join("\n");
  }
  if (text === "" || text.endsWith("\n")) return text + seed;
  return `${text}\n${seed}`;
}

/** The pills under the field for a feature or a standalone plan: the starters, then a document. */
export function KickoffStarters({
  disabled,
  onChoose,
  onAttach,
}: {
  disabled: boolean;
  onChoose: (starter: Starter) => void;
  onAttach: (document: StarterDocument) => void;
}) {
  return (
    <fieldset disabled={disabled} className="kickoff-starters">
      <legend className="kickoff-starters__label">Or begin with one of these</legend>
      {STARTERS.map((starter) => (
        <button
          key={starter.label}
          type="button"
          className="hearth-pill kickoff-starter"
          onClick={() => onChoose(starter)}
        >
          <starter.Icon aria-hidden="true" />
          {starter.label}
        </button>
      ))}
      <DocumentStarter onPick={onAttach} variant="ghost" className="hearth-pill kickoff-starter">
        <FileUp aria-hidden="true" />
        Start from a document I have
      </DocumentStarter>
    </fieldset>
  );
}

/**
 * The picked feature's open findings as toggle pills: pressing one puts its seed on a line of the
 * field, pressing it again takes that line out. A feature with none says so in one quiet line.
 */
export function FollowUpSuggestions({
  feature,
  text,
  disabled,
  onToggle,
}: {
  feature: FollowUpFeature;
  text: string;
  disabled: boolean;
  onToggle: (seed: string) => void;
}) {
  if (feature.candidates.length === 0) {
    return (
      <p className="kickoff-starters__label kickoff-starters__none">
        No open findings in <Untrusted text={feature.name} />
        {"'s run reviews."}
      </p>
    );
  }
  return (
    <fieldset disabled={disabled} className="kickoff-starters">
      <legend className="kickoff-starters__label">
        Open findings from <Untrusted text={feature.name} />
        {"'s reviews. Each one you pick goes into the field."}
      </legend>
      {feature.candidates.map((candidate, index) => {
        const seed = followUpSeed(candidate);
        const on = hasSeed(text, seed);
        return (
          <button
            key={`${candidate.stage_id}-${index}`}
            type="button"
            className="hearth-pill kickoff-starter"
            aria-pressed={on}
            onClick={() => onToggle(seed)}
          >
            {on ? <Check aria-hidden="true" /> : <Plus aria-hidden="true" />}
            <Untrusted text={candidate.text} />
          </button>
        );
      })}
    </fieldset>
  );
}
