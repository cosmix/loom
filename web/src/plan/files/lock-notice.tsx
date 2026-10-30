import type { JSX } from "react";

import { SentenceText } from "@/plan/concept/sentence-text";
import { OTHER_GLYPHS } from "@/plan/ui/glyphs";

const ReadOnlyGlyph = OTHER_GLYPHS.readOnly;

/** The `.lock` box: a muted 12.5px note with the read-only glyph. */
const LOCK_BOX =
  "flex items-start gap-2 rounded-lg bg-muted px-2.5 py-2 text-[12.5px] text-muted-foreground";

function LockGlyph(): JSX.Element {
  return <ReadOnlyGlyph aria-hidden="true" className="mt-0.5 size-3.5 shrink-0" />;
}

export function LockNotice({ notice }: { notice: string }): JSX.Element {
  return (
    <p role="note" className={LOCK_BOX}>
      <LockGlyph />
      <span style={{ unicodeBidi: "isolate", direction: "ltr" }}>
        <SentenceText text={notice} />
      </span>
    </p>
  );
}
