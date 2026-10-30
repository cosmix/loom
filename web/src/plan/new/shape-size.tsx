import type { KickoffSize } from "@/plan/api/schema-kickoff";
import { useProposalActions } from "@/plan/new/proposal";

const CHOICES: ReadonlyArray<{ size: KickoffSize; label: string; message: string }> = [
  { size: "feature", label: "Feature", message: "Make this a feature instead." },
  { size: "plan", label: "Standalone plan", message: "Make this a standalone plan instead." },
];

/**
 * Feature | Standalone plan. Neither is pressed until the conversation settles a size; a click
 * asks the session to change it, the same message the proposal's alternative sends.
 */
export function ShapeSize({ size }: { size: KickoffSize | null }) {
  const actions = useProposalActions();
  return (
    <>
      <div role="group" aria-label="Feature or standalone plan" className="kickoff-seg">
        {CHOICES.map((choice) => (
          <button
            key={choice.size}
            type="button"
            aria-pressed={size === choice.size}
            aria-disabled={actions.reason !== null || undefined}
            title={actions.reason ?? undefined}
            onClick={() => {
              if (size !== choice.size && actions.reason === null) actions.message(choice.message);
            }}
          >
            {choice.label}
          </button>
        ))}
      </div>
      {size === null && <p className="kickoff-muted">Not decided yet</p>}
    </>
  );
}
