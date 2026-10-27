import { CheckIcon, type LucideIcon } from "lucide-react";
import { type JSX, useEffect, useState } from "react";

import { Button } from "@/components/ui/button";

type CopyState = "idle" | "copied" | "failed";

/** Selects an element's text so a failed copy can be finished by hand. */
export function selectContents(element: HTMLElement | null): void {
  const selection = window.getSelection();
  if (!element || !selection) return;
  const range = document.createRange();
  range.selectNodeContents(element);
  selection.removeAllRanges();
  selection.addRange(range);
}

/**
 * A small icon button that copies `text`. The clipboard API is missing outside
 * secure contexts (a remote plain-HTTP bind), so a failure says so and hands
 * over to `onFailed`.
 */
export function CopyButton({
  text,
  label,
  icon: Icon,
  onFailed,
}: {
  text: string;
  label: string;
  icon: LucideIcon;
  onFailed?: () => void;
}): JSX.Element {
  const [state, setState] = useState<CopyState>("idle");

  useEffect(() => {
    if (state === "idle") return;
    const id = window.setTimeout(() => setState("idle"), 1500);
    return () => window.clearTimeout(id);
  }, [state]);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setState("copied");
    } catch {
      setState("failed");
      onFailed?.();
    }
  };

  const shown = state === "copied" ? "Copied" : state === "failed" ? "Copy failed" : label;
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-xs"
      className="md-copy"
      data-state={state}
      onClick={copy}
      aria-label={shown}
      title={shown}
    >
      {state === "copied" ? <CheckIcon /> : <Icon />}
    </Button>
  );
}
