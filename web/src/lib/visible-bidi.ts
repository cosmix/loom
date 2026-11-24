/** Bidirectional controls: ALM, LRM, RLM, embeddings and overrides, isolates. */
const BIDI_CONTROLS = /[\u061C\u200E\u200F\u202A-\u202E\u2066-\u2069]/gu;

/** `⟨U+XXXX⟩` for one character, so a hidden format character shows as text. */
export function codePointLabel(character: string): string {
  const hex = character.codePointAt(0)!.toString(16).toUpperCase().padStart(4, "0");
  return `⟨U+${hex}⟩`;
}

/**
 * Makes bidirectional controls visible. Other format characters (joiners in emoji sequences and
 * Persian or Indic text, soft hyphen, word joiner) are left alone because they change how
 * legitimate text is shaped.
 */
export function visibleBidi(text: string): string {
  return text.replace(BIDI_CONTROLS, codePointLabel);
}
