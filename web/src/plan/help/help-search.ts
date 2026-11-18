import { useEffect, useRef, useState, type RefObject } from "react";

import { isTyping } from "@/lib/is-typing";

export interface HelpSearch {
  query: string;
  setQuery: (query: string) => void;
  input: RefObject<HTMLInputElement | null>;
  onEscape: (event: KeyboardEvent) => void;
}

/** The search box's state, the `/` shortcut that focuses it, and Esc clearing it before closing. */
export function useHelpSearch(open: boolean): HelpSearch {
  const [query, setQuery] = useState("");
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing || event.key !== "/") return;
      if (event.metaKey || event.ctrlKey || event.altKey || isTyping(event.target)) return;
      event.preventDefault();
      input.current?.focus();
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [open]);
  const onEscape = (event: KeyboardEvent) => {
    if (document.activeElement !== input.current || !query) return;
    event.preventDefault();
    setQuery("");
  };
  return { query, setQuery, input, onEscape };
}
