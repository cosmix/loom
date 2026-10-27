import type { Sentence } from "@/plan/api/schema-concept";
import { MAX_ATTACHMENT_BYTES } from "@/plan/attachments";
import type { AttachmentDraft } from "@/plan/attachments/types";

const encoder = new TextEncoder();

/** The sentence a `data-summary` / `data-sentence-id` pair names, with the hash of its page. */
export type SentenceResolver = (
  summary: string,
  id: string,
) => { sentence: Sentence; hash: string | null } | undefined;

const OWNER = "[data-sentence-id][data-summary], [data-passage]";
type Part = { owner: Element; text: string; nodes: Text[] };

function selectedParts(range: Range): Part[] {
  const root = range.commonAncestorContainer;
  const walker = root.ownerDocument!.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const parts: Part[] = [];
  let node: Node | null = root.nodeType === Node.TEXT_NODE ? root : walker.nextNode();
  while (node) {
    const text = node as Text;
    const owner = text.parentElement?.closest(OWNER);
    if (owner && range.intersectsNode(text)) {
      const start = range.startContainer === text ? range.startOffset : 0;
      const end = range.endContainer === text ? range.endOffset : text.length;
      const value = text.data.slice(start, end);
      if (value) {
        const last = parts[parts.length - 1];
        if (last?.owner === owner) {
          last.text += value;
          last.nodes.push(text);
        } else {
          parts.push({ owner, text: value, nodes: [text] });
        }
      }
    }
    node = root.nodeType === Node.TEXT_NODE ? null : walker.nextNode();
  }
  return parts.filter((part) => part.text.trim().length > 0);
}

function lineNumber(value: string | null): number | null {
  if (!value || !/^\d+$/.test(value)) return null;
  const number = Number(value);
  return Number.isSafeInteger(number) && number > 0 ? number : null;
}

function passageLines(part: Part): { start: number; end: number } | null {
  const blocks = part.nodes.map((node) => node.parentElement?.closest("[data-line]"));
  const block = blocks[0];
  if (block && part.owner.contains(block) && blocks.every((item) => item === block)) {
    const line = lineNumber(block.getAttribute("data-line"));
    if (line !== null) return { start: line, end: line };
  }
  const start = lineNumber(part.owner.getAttribute("data-start-line"));
  const end = lineNumber(part.owner.getAttribute("data-end-line"));
  return start !== null && end !== null && end >= start ? { start, end } : null;
}

function draftForPart(part: Part, resolve: SentenceResolver): AttachmentDraft | null {
  const passage = part.owner.getAttribute("data-passage");
  if (passage !== null) {
    const separator = passage.indexOf("#");
    if (separator <= 0) return null;
    return {
      kind: "source",
      path: passage.slice(0, separator),
      anchor: passage.slice(separator + 1),
      text: part.text,
      lines: passageLines(part),
      stageId: null,
      hash: part.owner.getAttribute("data-hash"),
    };
  }
  const summary = part.owner.getAttribute("data-summary");
  const id = part.owner.getAttribute("data-sentence-id");
  if (!summary || !id) return null;
  const found = resolve(summary, id);
  if (!found) return null;
  return {
    kind: "summary",
    summary,
    sentence: id,
    text: part.text,
    restsOn: found.sentence.rests_on,
    hash: found.hash,
  };
}

/**
 * One draft per selected part, in reading order, so a selection spanning levels carries each
 * part's provenance. Passage blocks supply their document hash in `data-hash`; a summary
 * sentence is resolved by (summary, id) through `resolve`, because an opened level-1 run shows
 * another summary's sentences whose ids can collide with the page's. A part that does not
 * resolve is dropped.
 */
export function attachmentsFromSelection(
  sel: Selection,
  resolve: SentenceResolver,
): AttachmentDraft[] {
  if (sel.rangeCount === 0 || sel.isCollapsed) return [];
  return selectedParts(sel.getRangeAt(0)).flatMap((part) => draftForPart(part, resolve) ?? []);
}

/** Whether a draft is over plan 3's per-attachment limit (section 12, 16 KiB). */
export function overLimit(draft: AttachmentDraft): boolean {
  return encoder.encode(draft.text).length > MAX_ATTACHMENT_BYTES;
}

/**
 * The draft plan 3's single-draft `useSelectionAttachment` shows: the first part over the limit,
 * so Add to chat shows its note and adds nothing, else the first part.
 */
export function attachmentFromSelection(
  sel: Selection,
  resolve: SentenceResolver,
): AttachmentDraft | null {
  const parts = attachmentsFromSelection(sel, resolve);
  return parts.find(overLimit) ?? parts[0] ?? null;
}
