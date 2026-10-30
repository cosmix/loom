import { Layers } from "lucide-react";
import type { CSSProperties, JSX } from "react";

import "./feature-card.css";

const DOC_COLOURS = 8;

/** The same slug always lands on the same document colour, 1 through 8. */
export function featureColour(slug: string): number {
  let hash = 5381;
  for (const unit of slug) hash = (Math.imul(hash, 33) + unit.codePointAt(0)!) >>> 0;
  return 1 + (hash % DOC_COLOURS);
}

/** A DocMark-sized square for a feature, tinted by a hash of its slug over `--doc-1..8`. */
export function FeatureMark({ slug }: { slug: string }): JSX.Element {
  const style = { "--mark": `var(--doc-${featureColour(slug)})` } as CSSProperties;

  return (
    <span aria-hidden="true" className="feature-mark" style={style}>
      <Layers className="size-4" />
    </span>
  );
}
