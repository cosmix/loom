const KICKOFF_INFO = "loom-kickoff";
const OPENING = /^ {0,3}(`{3,}|~{3,})(.*)$/;

type Fence = { marker: string; named: boolean; body: string[] };
type Scan = { kept: string[]; blocks: Array<{ body: string[]; closed: boolean }> };

function opening(line: string, info: string): Fence | null {
  const match = OPENING.exec(line);
  if (!match) return null;
  const marker = match[1]!;
  const found = match[2]!.trim();
  // A backtick fence's info string holds no backtick: that line is inline code, not a fence.
  if (marker.startsWith("`") && found.includes("`")) return null;
  return { marker, named: found === info, body: [] };
}

function closes(line: string, fence: Fence): boolean {
  const run = new RegExp(`^ {0,3}${fence.marker[0]}{${fence.marker.length},}\\s*$`);
  return run.test(line);
}

/** Walks the text once: lines outside every fence named `info` are kept, each named fence is a block. */
function scan(markdown: string, info: string): Scan {
  const result: Scan = { kept: [], blocks: [] };
  let fence: Fence | null = null;
  let block: Scan["blocks"][number] | null = null;
  for (const line of markdown.split("\n")) {
    if (fence === null) {
      fence = opening(line, info);
      if (fence?.named) {
        block = { body: fence.body, closed: false };
        result.blocks.push(block);
      } else result.kept.push(line);
      continue;
    }
    const done = closes(line, fence);
    if (!fence.named) result.kept.push(line);
    else if (!done) fence.body.push(line);
    if (done) {
      if (block && fence.named) block.closed = true;
      fence = null;
      block = null;
    }
  }
  return result;
}

/**
 * The assistant ends a reply with a fenced JSON block that the server (or the closing card) reads:
 * `loom-kickoff` feeds the Taking shape panel, `loom-closing` the card of what an ended chat left
 * behind. It is instructions to the program, so the transcript never draws it. A fence whose info
 * string is exactly `info` goes, through its closing fence or the end of the text when it is still
 * streaming; every other fence, and the prose around the block, stays. A block quoted inside a
 * longer fence is part of that code and stays too.
 */
export function withoutFence(markdown: string, info: string): string {
  if (!markdown.includes(info)) return markdown;
  const { kept, blocks } = scan(markdown, info);
  return blocks.length > 0 ? kept.join("\n").trimEnd() : markdown;
}

/** The body of the last fence whose info string is exactly `info`; null when none or still open. */
export function fenceBody(markdown: string, info: string): string | null {
  if (!markdown.includes(info)) return null;
  const last = scan(markdown, info).blocks.at(-1);
  return last?.closed ? last.body.join("\n") : null;
}

export function withoutKickoffFence(markdown: string): string {
  return withoutFence(markdown, KICKOFF_INFO);
}
