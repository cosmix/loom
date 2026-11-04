import type { Attachment } from "@/plan/api/schema-chat";
import { parsePageKey } from "@/plan/api/page-key";
import { docRel } from "@/plan/files/address";

export type LabelInput = Pick<
  Attachment,
  "path" | "anchor" | "lines" | "stage" | "summary_sentence" | "kind" | "quote" | "file"
>;

/** How much of a selection's quote its chip shows. */
const QUOTE_CHARS = 48;

/**
 * A selection (a summary sentence, or a passage of a source) is named by what was selected, so two
 * selections on one page read differently. A whole document keeps its file name.
 */
function isSelection(a: LabelInput): boolean {
  if (a.quote.trim() === "") return false;
  return a.kind === "summary" || a.anchor !== null || a.lines !== null || a.stage !== null;
}

/** The quote's first 48 characters on one line, whitespace collapsed; an ellipsis marks a cut. */
function quoteStart(quote: string): string {
  const chars = Array.from(quote.replace(/\s+/g, " ").trim());
  return chars.length > QUOTE_CHARS
    ? `“${chars.slice(0, QUOTE_CHARS).join("").trimEnd()}…”`
    : `“${chars.join("")}”`;
}

function sourceLabel(a: LabelInput): string {
  const file = docRel(a.path).split("/").pop() ?? a.path;
  const passage = a.stage ? ` › stage ${a.stage}` : a.anchor ? ` › ${a.anchor}` : "";
  const lines = a.lines ? ` · lines ${a.lines[0]}–${a.lines[1]}` : "";
  return `${file}${passage}${lines}`;
}

/** "Product overview" or "Engineering overview" for an overview summary, else null. */
function overviewName(a: LabelInput): string | null {
  const page = parsePageKey(summaryIdOf(a.path));
  if (page?.kind !== "overview") return null;
  return page.scope === "product" ? "Product overview" : "Engineering overview";
}

function summaryStem(a: LabelInput): string {
  const file = docRel(a.path).split("/").pop() ?? a.path;
  return file.replace(/\.md$/, "");
}

/** What the chip says: a file's name, a selection's quoted start, else where the document is. */
export function chipLabel(a: LabelInput): string {
  if (a.kind === "file") return a.file?.name ?? "File";
  if (isSelection(a)) return quoteStart(a.quote);
  if (a.kind === "summary") return overviewName(a) ?? `Summary · ${summaryStem(a)}`;
  return sourceLabel(a);
}

/** The chip's secondary text: where a selection came from; null when the label says it already. */
export function chipSource(a: LabelInput): string | null {
  if (a.kind === "file" || !isSelection(a)) return null;
  return a.kind === "summary" ? (overviewName(a) ?? `${summaryStem(a)} summary`) : sourceLabel(a);
}

/** The summary id a `doc/loom/summaries/<id>.md` path names. */
export function summaryIdOf(path: string): string {
  return docRel(path)
    .replace(/^loom\/summaries\//, "")
    .replace(/\.md$/, "");
}
