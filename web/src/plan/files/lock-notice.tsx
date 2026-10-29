import type { JSX } from "react";

import { SentenceText } from "@/plan/concept/sentence-text";
import { OTHER_GLYPHS } from "@/plan/ui/glyphs";

const ReadOnlyGlyph = OTHER_GLYPHS.readOnly;

export function LockNotice({ notice }: { notice: string }): JSX.Element {
  return (
    <p role="note" className="flex items-start gap-1.5 text-xs text-muted-foreground">
      <ReadOnlyGlyph aria-hidden="true" className="mt-0.5 size-3 shrink-0" />
      <span>
        <SentenceText text={notice} />
      </span>
    </p>
  );
}
