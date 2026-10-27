import { SearchIcon } from "lucide-react";
import { useEffect, useRef, type JSX, type RefObject } from "react";

import { cn } from "cn";

import { isTyping } from "@/lib/is-typing";

export const SEARCH_SCOPES = [
  ["product", "Product"],
  ["plans", "Plans"],
  ["knowledge", "Knowledge"],
  ["all", "All"],
] as const;
export type SearchScope = (typeof SEARCH_SCOPES)[number][0];
export const MAX_QUERY = 256;

/** `/` focuses the input unless the keystroke is already going into a field. */
function useSlashFocus(input: RefObject<HTMLInputElement | null>): void {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.altKey || event.ctrlKey || event.metaKey) return;
      if (isTyping(event.target)) return;
      event.preventDefault();
      input.current?.focus();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [input]);
}

function ScopePicker({
  scope,
  onScope,
}: {
  scope: SearchScope;
  onScope: (scope: SearchScope) => void;
}): JSX.Element {
  return (
    <div role="group" aria-label="Search scope" className="flex gap-1">
      {SEARCH_SCOPES.map(([id, label]) => (
        <button
          key={id}
          type="button"
          aria-pressed={scope === id}
          className={cn(
            "rounded-md px-2 py-0.5 text-xs font-medium",
            scope === id ? "bg-muted text-foreground" : "text-muted-foreground",
          )}
          onClick={() => onScope(id)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

type Props = {
  value: string;
  scope: SearchScope;
  onChange: (value: string) => void;
  onScope: (scope: SearchScope) => void;
  onSubmit: () => void;
  onClear: () => void;
};

/**
 * Filters the tree as you type; Enter searches the text. `/` focuses, Esc clears. An Esc that
 * cleared something stops here, so a drawer around the box stays open; on an empty box it passes on.
 */
export function SearchBox({
  value,
  scope,
  onChange,
  onScope,
  onSubmit,
  onClear,
}: Props): JSX.Element {
  const input = useRef<HTMLInputElement>(null);
  useSlashFocus(input);

  return (
    <div className="flex flex-col gap-2 border-b border-hairline p-3">
      <label className="relative block">
        <span className="sr-only">Search documents</span>
        <SearchIcon
          className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground"
          aria-hidden="true"
        />
        <input
          ref={input}
          type="search"
          value={value}
          maxLength={MAX_QUERY}
          placeholder="Filter, or press Enter to search"
          className="h-8 w-full rounded-md border border-border bg-transparent pr-2 pl-7 text-[13px] outline-none focus-visible:border-ring"
          onChange={(event) => onChange(event.target.value.slice(0, MAX_QUERY))}
          onKeyDown={(event) => {
            if (event.key === "Enter" && value.trim()) onSubmit();
            else if (event.key === "Escape" && value !== "") {
              event.stopPropagation();
              onClear();
            }
          }}
        />
      </label>
      {value.trim() && <ScopePicker scope={scope} onScope={onScope} />}
    </div>
  );
}
