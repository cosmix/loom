import { createElement, type JSX } from "react";

export function visibleControls(text: string): string {
  return text.replace(/\p{Cf}/gu, (character) => {
    const hex = character.codePointAt(0)!.toString(16).toUpperCase().padStart(4, "0");
    return `⟨U+${hex}⟩`;
  });
}

export function Untrusted(props: { text: string }): JSX.Element {
  return createElement(
    "span",
    { style: { unicodeBidi: "plaintext", direction: "ltr" } },
    visibleControls(props.text),
  );
}
