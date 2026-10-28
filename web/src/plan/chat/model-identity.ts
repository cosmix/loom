import type { ChatSnapshot } from "@/plan/api/schema-chat";
import { attachmentAddress, quoteLines } from "./format-attachments";

/** The control set plan 7 strips (plan_session/input.rs `sanitize`): C0 but tab and newline, DEL, C1. */
// oxlint-disable-next-line no-control-regex -- the class is the control set plan 7 strips
export const CONTROLS = /[\u0000-\u0008\u000b-\u001f\u007f-\u009f]/g;

type Projected = {
  kind: string;
  path: string;
  anchor: string | null;
  stage: string | null;
  summary_sentence: string | null;
  lines: readonly [number, number] | null;
  quote: string;
};
export type MessageContent = { text: string; attachments?: readonly Projected[] };

/**
 * The quote as the transcript's item carries it: format.rs cuts it at 2,000 characters (a cut quote
 * keeps its marker line, and cutting it again changes nothing), then dispatch strips controls from
 * the whole formatted text. The raw quote of a send, the server's `s-` item and the delivered `u-`
 * item all normalise to the same string.
 */
const deliveredQuote = (quote: string) => quoteLines(quote).join("\n").replace(CONTROLS, "");

/**
 * The one message identity: the text as the server sanitises it, plus each attachment's kind,
 * path, address, lines and quote. Only what survives the round trip through the delivered block
 * (attachments/format.rs, parse.rs) can be compared with the transcript's item: the address is
 * the one format.rs writes (`stage:<id>`, else the anchor, else the summary sentence), which
 * parse.rs reads back as the anchor. The quote is compared as delivered (`deliveredQuote`): a
 * rendered-view selection has no lines, so two passages of one section differ by their quote only.
 * The hash, lock and changed flags are left out; a delivered item does not carry them. Every
 * matcher goes through this.
 */
export const messageIdentity = ({ text, attachments = [] }: MessageContent) =>
  JSON.stringify([
    text.replace(CONTROLS, ""),
    attachments.map((attachment) => [
      attachment.kind,
      attachment.path,
      attachmentAddress(attachment),
      attachment.lines,
      deliveredQuote(attachment.quote),
    ]),
  ]);
export const sameMessage = (a: MessageContent, b: MessageContent) =>
  messageIdentity(a) === messageIdentity(b);

/** Ids of the queued messages of the current chat with this text; a queue entry carries no attachments. */
export function queuedIds(queue: ChatSnapshot["queue"], text: string): string[] {
  const key = messageIdentity({ text });
  return queue
    .filter((queued) => queued.text !== null && messageIdentity({ text: queued.text }) === key)
    .map((queued) => queued.id);
}

/**
 * The send is in the server's queue: a message with its text waits that was not queued when the
 * send was issued. Queue ids are unique, so this stays true when an earlier copy leaves the queue.
 */
export const isQueued = (
  send: { text: string; knownQueuedIds?: readonly string[] },
  queue: ChatSnapshot["queue"],
) => queuedIds(queue, send.text).some((id) => !send.knownQueuedIds?.includes(id));
