import { useAtomValue } from "jotai/react";

import { canAuthorAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";

/** Section 01's offer labels; an id outside the table falls back to a generic label. */
const OFFER_LABELS: Record<string, string> = {
  "start-vision": "Start the product vision",
  "start-metaplan": "Start a metaplan from the product spec",
};

/** `DisabledReason` never calls it while its reason is set, which is always here. */
function blockedClick(): void {}

/**
 * An empty-state offer. Chat does not exist yet, so the button stays disabled with its reason
 * as the accessible description (plan 9 replaces the body, keeping `{ offer }`). A browser
 * that cannot author sees the label as text.
 */
export function EmptyOffer({ offer }: { offer: string }) {
  const canAuthor = useAtomValue(canAuthorAtom);
  const label = OFFER_LABELS[offer] ?? "Start";
  if (!canAuthor) return <span className="text-[13px] text-muted-foreground">{label}</span>;
  return (
    <DisabledReason reason="Available once chat is built" onClick={blockedClick}>
      {label}
    </DisabledReason>
  );
}
