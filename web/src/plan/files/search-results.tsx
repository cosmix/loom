import type { JSX } from "react";

import type { SearchHit, SearchResults as Results } from "@/plan/api/schema-files";
import { ISOLATED_LTR, Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { escapeRegExp } from "@/plan/markdown/context";

/** The same highlight the rendered document gives a mark (`.md mark`), outside `.md` here. */
const MARK = "rounded-[2px] bg-(--highlight) px-px text-(color:--highlight-foreground)";

/** The snippet as text with the terms wrapped in `<mark>`; server text never becomes HTML. */
function Marked({ text, terms }: { text: string; terms: string[] }): JSX.Element {
  const usable = terms.filter(Boolean);
  if (usable.length === 0) return <>{visibleControls(text)}</>;
  const pattern = new RegExp(`(${usable.map(escapeRegExp).join("|")})`, "gi");
  const wanted = new Set(usable.map((term) => term.toLowerCase()));
  return (
    <>
      {text.split(pattern).map((part, index) =>
        wanted.has(part.toLowerCase()) ? (
          <mark key={index} className={MARK}>
            {visibleControls(part)}
          </mark>
        ) : (
          visibleControls(part)
        ),
      )}
    </>
  );
}

type Props = {
  results: Results;
  onOpen: (hit: SearchHit) => void;
};

export function SearchResults({ results, onOpen }: Props): JSX.Element {
  if (results.hits.length === 0) {
    return <p className="p-3 text-[13px] text-muted-foreground">No results.</p>;
  }
  return (
    <div>
      <ul aria-label="Search results" className="divide-y divide-hairline">
        {results.hits.map((hit, index) => (
          <li key={`${hit.path}#${hit.anchor}:${index}`}>
            <button
              type="button"
              className="flex w-full flex-col gap-0.5 px-3 py-2 text-left hover:bg-muted"
              onClick={() => onOpen(hit)}
            >
              <span className="text-[13px] font-medium">
                <Untrusted text={hit.title} />
              </span>
              {hit.heading_path.length > 0 && (
                <span className="text-xs text-muted-foreground">
                  <Untrusted text={hit.heading_path.join(" › ")} />
                </span>
              )}
              <span className="text-xs" style={ISOLATED_LTR}>
                <Marked text={hit.snippet} terms={hit.terms} />
              </span>
            </button>
          </li>
        ))}
      </ul>
      {results.truncated && (
        <p className="p-3 text-xs text-muted-foreground">50 results shown; refine the search</p>
      )}
    </div>
  );
}
