import { createElement, type JSX } from "react";

import { codePointLabel } from "@/lib/visible-bidi";

export function visibleControls(text: string): string {
  return text.replace(/\p{Cf}/gu, codePointLabel);
}

/** The style that keeps server text out of the surrounding text's bidirectional reordering. */
export const ISOLATED_LTR = { unicodeBidi: "isolate", direction: "ltr" } as const;

/**
 * A left-to-right isolated block (section 12). `plaintext` would lay a paragraph that starts with
 * an RTL letter out right-to-left and ignore `direction`, so a command's segments could display in
 * a different order from the one that runs; `isolate` keeps the order and the surrounding text out
 * of the reordering.
 */
export function Untrusted(props: { text: string }): JSX.Element {
  return createElement("span", { style: ISOLATED_LTR }, visibleControls(props.text));
}
