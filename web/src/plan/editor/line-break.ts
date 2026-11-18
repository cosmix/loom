// Pure (no CodeMirror): which line separator makes the editor's text round-trip byte for byte.
// CodeMirror splits on "\n", "\r" and "\r\n" and joins with "\n", so a CRLF file would be saved
// as LF unless the editor is told to split on the file's own break.

/**
 * `null` when `text` has no "\r" (the default splitting is already exact). Otherwise the one
 * break every line uses when they are all "\r\n" or all "\r"; for a mix it is "\n", so each
 * stray "\r" stays a character of its line.
 */
export function detectLineBreak(text: string): string | null {
  if (!text.includes("\r")) return null;
  const loneLf = /(?<!\r)\n/.test(text);
  const loneCr = /\r(?!\n)/.test(text);
  const crlf = /\r\n/.test(text);
  if (loneLf || (loneCr && crlf)) return "\n";
  return loneCr ? "\r" : "\r\n";
}

/** `text` with every "\r\n", "\r" and "\n" replaced by `lineBreak`. */
export function withLineBreak(text: string, lineBreak: string): string {
  return text.replace(/\r\n?|\n/g, lineBreak);
}
