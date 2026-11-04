const KICKOFF_INFO = "loom-kickoff";
const OPENING = /^ {0,3}(`{3,}|~{3,})(.*)$/;

type Fence = { marker: string; kickoff: boolean };

function opening(line: string): Fence | null {
  const match = OPENING.exec(line);
  if (!match) return null;
  const marker = match[1]!;
  const info = match[2]!.trim();
  // A backtick fence's info string holds no backtick: that line is inline code, not a fence.
  if (marker.startsWith("`") && info.includes("`")) return null;
  return { marker, kickoff: info === KICKOFF_INFO };
}

function closes(line: string, fence: Fence): boolean {
  const run = new RegExp(`^ {0,3}${fence.marker[0]}{${fence.marker.length},}\\s*$`);
  return run.test(line);
}

/**
 * The assistant ends each kickoff reply with a fenced `loom-kickoff` JSON block that the server
 * parses into the Taking shape panel. It is instructions to the server, so the transcript never
 * draws it. A fence whose info string is exactly `loom-kickoff` goes, through its closing fence or
 * the end of the text when it is still streaming; every other fence, and the prose around the
 * block, stays. A kickoff block quoted inside a longer fence is part of that code and stays too.
 */
export function withoutKickoffFence(markdown: string): string {
  if (!markdown.includes(KICKOFF_INFO)) return markdown;
  const kept: string[] = [];
  let fence: Fence | null = null;
  let dropped = false;
  for (const line of markdown.split("\n")) {
    if (fence === null) {
      fence = opening(line);
      if (fence?.kickoff) dropped = true;
      else kept.push(line);
      continue;
    }
    if (!fence.kickoff) kept.push(line);
    if (closes(line, fence)) fence = null;
  }
  return dropped ? kept.join("\n").trimEnd() : markdown;
}
