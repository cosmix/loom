import type { Paragraph } from "mdast";

// A metaplan entry's `Covers:` line as data: its tokens, grouped by the product section each
// names, with every heading resolved to its title through a lookup the host supplies. The line's
// format is the trace's (`loom/src/planning/trace/tokens.rs`); this port reads only the section
// tokens, so a change to their syntax there changes `parseCoverToken` here.

const PREFIX = "Covers:";

/** One product section as the host knows it. */
export type CoverSection = {
  /** Repo-relative path of the section file. */
  path: string;
  /** The section's title without its leading number. */
  title: string;
  /** Heading titles by anchor. */
  headings: ReadonlyMap<string, string>;
  /** False while more headings may still arrive: an anchor not in `headings` may yet resolve. */
  complete: boolean;
};

/** The section numbered `number`, or null when the feature has no such section. */
export type CoverLookup = (number: number) => CoverSection | null;

/** What a `Covers:` line renders from: the lookup and the source its paragraphs are cut from. */
export type CoversSource = { lookup: CoverLookup; source: string };

/** A token as written, and the section and anchor it names when it is a section token. */
export type CoverToken = { raw: string; section: number | null; anchor: string | null };

export type CoverRef =
  | { kind: "heading"; path: string; anchor: string; title: string }
  | { kind: "pending" | "missing"; text: string; raw: string };

export type CoverGroup = {
  /** The section number, or null for the tokens that name no section. */
  number: number | null;
  section: Pick<CoverSection, "path" | "title"> | null;
  /** The line names the section whole. */
  whole: boolean;
  refs: CoverRef[];
};

/**
 * The value after `Covers:` when `node` is a whole `Covers:` line at column 0, else null. A
 * paragraph that runs on past its first line is left as prose: the trace reads one line only.
 * Fenced code is a `code` node, never a paragraph, so a fenced example never matches.
 */
export function coversValue(node: Paragraph, source: string): string | null {
  const position = node.position;
  if (!position || position.start.column !== 1 || position.start.line !== position.end.line) {
    return null;
  }
  const { offset: start } = position.start;
  const { offset: end } = position.end;
  if (start === undefined || end === undefined) return null;
  const text = source.slice(start, end);
  return text.startsWith(PREFIX) ? text.slice(PREFIX.length) : null;
}

/** A heading anchor the trace accepts: lowercase letters, digits, `-` and `_`, or `stage:<id>`. */
function validAnchor(anchor: string): boolean {
  if (anchor.startsWith("stage:")) return /^[A-Za-z0-9_-]+$/.test(anchor.slice(6));
  return /^[\p{L}\p{N}_-]+$/u.test(anchor) && !/\p{Lu}/u.test(anchor);
}

function parsePlain(raw: string): Pick<CoverToken, "section" | "anchor"> | null {
  const hash = raw.indexOf("#");
  const head = hash === -1 ? raw : raw.slice(0, hash);
  const anchor = hash === -1 ? null : raw.slice(hash + 1);
  if (anchor !== null && !validAnchor(anchor)) return null;
  const digits = /^section_(\d+)$/.exec(head.replace(/\.md$/, ""));
  return digits ? { section: Number(digits[1]), anchor } : null;
}

/** `[label](target)`: the target's file name and fragment win, then the label. */
function parseLink(raw: string): Pick<CoverToken, "section" | "anchor"> | null {
  const link = /^\[(.*)\]\((.*)\)$/.exec(raw);
  if (!link) return null;
  const [, label = "", target = ""] = link;
  const [path = "", fragment] = target.split("#", 2);
  const name = path.split("?")[0]?.split("/").at(-1) ?? "";
  const fromTarget = name === "" ? null : parsePlain(fragment ? `${name}#${fragment}` : name);
  return fromTarget ?? parsePlain(label.trim());
}

export function parseCoverToken(raw: string): CoverToken {
  const named = raw.startsWith("[") ? parseLink(raw) : parsePlain(raw);
  return { raw, section: named?.section ?? null, anchor: named?.anchor ?? null };
}

/** The line's tokens in order, without empty ones or repeats. */
export function parseCovers(value: string): CoverToken[] {
  const seen = new Set<string>();
  const tokens: CoverToken[] = [];
  for (const raw of value.split(",").map((part) => part.trim())) {
    const token = parseCoverToken(raw);
    const key = token.section === null ? `raw:${raw}` : `${token.section}#${token.anchor ?? ""}`;
    if (raw === "" || seen.has(key)) continue;
    seen.add(key);
    tokens.push(token);
  }
  return tokens;
}

function headingRef(token: CoverToken, anchor: string, section: CoverSection | null): CoverRef {
  const title = section?.headings.get(anchor);
  if (section && title !== undefined) return { kind: "heading", path: section.path, anchor, title };
  const pending = section !== null && !section.complete;
  return {
    kind: pending ? "pending" : "missing",
    text: section ? anchor : token.raw,
    raw: token.raw,
  };
}

/**
 * The tokens grouped by section in section order, each group's headings in line order; the
 * tokens that name no section come last, as written.
 */
export function groupCovers(tokens: readonly CoverToken[], lookup: CoverLookup): CoverGroup[] {
  const groups = new Map<number | null, CoverGroup>();
  for (const token of tokens) {
    const number = token.section;
    let group = groups.get(number);
    const section = number === null ? null : lookup(number);
    if (!group) {
      group = {
        number,
        section: section && { path: section.path, title: section.title },
        whole: false,
        refs: [],
      };
      groups.set(number, group);
    }
    if (number === null) group.refs.push({ kind: "missing", text: token.raw, raw: token.raw });
    else if (token.anchor === null) group.whole = true;
    else group.refs.push(headingRef(token, token.anchor, section));
  }
  return [...groups.values()].sort(
    (a, b) => (a.number ?? Number.MAX_SAFE_INTEGER) - (b.number ?? Number.MAX_SAFE_INTEGER),
  );
}
