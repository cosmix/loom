import type { Attachment } from "@/plan/api/schema-chat";

const OPEN_TAG = "<loom-attachments>";
const CLOSE_TAG = "</loom-attachments>";
const QUOTE_CHARS = 2_000;
const CUT_MARKER = /^\[…[0-9]+ more characters; read the source\]$/;

type Addressed = {
  anchor: string | null;
  stage: string | null;
  summary_sentence: string | null;
};

/** The address format.rs writes after the path: `stage:<id>`, else the anchor, else the sentence. */
export function attachmentAddress({ anchor, stage, summary_sentence }: Addressed): string | null {
  return stage !== null ? `stage:${stage}` : (anchor ?? summary_sentence);
}

export function formatMessage(text: string, attachments: Attachment[]): string {
  const escaped = escapeText(text);
  if (attachments.length === 0) return escaped;

  const block = [OPEN_TAG];
  for (const [index, attachment] of attachments.entries()) {
    block.push(header(index + 1, attachment));
    for (const line of quoteLines(attachment.quote)) block.push(`> ${line}`);
  }
  block.push(CLOSE_TAG);
  return `${escaped}\n\n${block.join("\n")}`;
}

export function deliveredBytes(text: string, attachments: Attachment[]): number {
  return new TextEncoder().encode(formatMessage(text, attachments)).byteLength;
}

const WHITE_SPACE = /^\p{White_Space}$/u;

// A single linear pass from each end; a global trailing-run regex is quadratic on long
// whitespace runs and this runs on every keystroke. White_Space is BMP-only, so UTF-16 units suffice.
function trimWhiteSpace(text: string): string {
  let start = 0;
  let end = text.length;
  while (start < end && WHITE_SPACE.test(text[start])) start += 1;
  while (end > start && WHITE_SPACE.test(text[end - 1])) end -= 1;
  return text.slice(start, end);
}

function escapeText(text: string): string {
  return text
    .split("\n")
    .map((line) => {
      // Rust ignores Cc controls except tab before applying Unicode White_Space trim.
      // JS trim() differs for U+0085 and U+FEFF, so use the Unicode property explicitly.
      // This class differs from CONTROLS (model-identity.ts) on purpose: it also strips newline (\u000a),
      // because Rust's tag_line ignores every Cc control but tab when it matches a tag line.
      // oxlint-disable-next-line no-control-regex -- the class is the control set Rust drops
      const visible = line.replace(/[\u0000-\u0008\u000a-\u001f\u007f-\u009f]/g, "");
      const trimmed = trimWhiteSpace(visible);
      const bare = trimmed.replace(/^\\+/, "");
      if (bare !== OPEN_TAG && bare !== CLOSE_TAG) return line;

      const offset = line.search(/[^\p{White_Space}\p{Cc}]/u);
      return offset < 0 ? line : `${line.slice(0, offset)}\\${line.slice(offset)}`;
    })
    .join("\n");
}

function header(number: number, attachment: Attachment): string {
  const anchor = attachmentAddress(attachment);
  const address = anchor === null ? attachment.path : `${attachment.path}#${anchor}`;
  let result = `[${number}] ${attachment.kind} ${address}`;
  if (attachment.lines !== null) {
    result += ` lines ${attachment.lines[0]}-${attachment.lines[1]}`;
  }
  if (attachment.rests_on.length > 0) result += ` rests-on ${attachment.rests_on.join(",")}`;
  if (attachment.locked !== null) result += ` locked=${attachment.locked}`;
  if (attachment.changed_since_selected) result += " changed-since-selected";
  return result;
}

function scalarValues(text: string): string[] {
  const values: string[] = [];
  for (const value of text) values.push(value);
  return values;
}

/** A quote as format.rs writes it: cut at a line boundary within 2,000 characters, marker appended. */
export function quoteLines(quote: string): string[] {
  const lines = quote.split("\n");
  const characters = scalarValues(quote);
  const total = characters.length;
  if (total <= QUOTE_CHARS || CUT_MARKER.test(lines[lines.length - 1])) return lines;

  let kept = 0;
  let shown = 0;
  for (const line of lines) {
    const width = scalarValues(line).length + Number(kept > 0);
    if (shown + width > QUOTE_CHARS) break;
    shown += width;
    kept += 1;
  }
  const out = lines.slice(0, kept);
  if (kept === 0) {
    out.push(characters.slice(0, QUOTE_CHARS).join(""));
    shown = QUOTE_CHARS;
  }
  out.push(`[…${total - shown} more characters; read the source]`);
  return out;
}
