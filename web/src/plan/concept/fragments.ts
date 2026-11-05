import type { Citation, ConceptPage, Sentence } from "@/plan/api/schema-concept";

import { passageAnchors, sentenceCitations, type SentenceSite } from "./column-context";
import { anchorKey, type CitedLookup } from "./cited-lookup";
import type { OpenRun } from "./types";

/*
 * A fragment is a run of consecutive sentences that open to the same content. The column reads
 * a fragment as one sentence: `mergeFragments` builds that sentence once per page, and the
 * existing machinery (open runs, reading order, carry, Open all, keys) needs no other change.
 * Grouping happens here on the client because the server cannot know adjacency inside an
 * opened run, where cited sentences from several summaries stand side by side.
 */

type SentenceCitation = ReturnType<typeof sentenceCitations>[number];

const MEMBERS = new WeakMap<Sentence, Sentence[]>();
const VIEWS = new WeakMap<ConceptPage, ConceptPage>();

/**
 * What opening `sentence` brings in: one key per cited sentence, else one per source section.
 * A sentence with no citations is not a button and has no keys, so it never joins a fragment.
 */
export function openKeys(sentence: Sentence): string[] {
  if (sentence.citations.length === 0) return [];
  const cited = sentenceCitations(sentence);
  if (cited.length > 0) return cited.map(citationKey);
  return passageAnchors(sentence).map((anchor) => `p|${anchorKey(anchor)}`);
}

function unionBy<T>(items: T[], keyOf: (item: T) => string): T[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    const key = keyOf(item);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function citationKey(citation: Citation): string {
  return citation.kind === "sentence"
    ? `s|${citation.summary}#${citation.id}`
    : `p|${citation.path}#${citation.anchor}`;
}

/** One sentence standing for `members`: the lead's id, the joined text, the unions of the rest. */
function merged(members: Sentence[]): Sentence {
  const lead = members[0]!;
  const sentence: Sentence = {
    id: lead.id,
    text: members.map((member) => member.text).join(" "),
    marks: unionBy(
      members.flatMap((member) => member.marks),
      (mark) => mark.doc.id,
    ),
    citations: unionBy(
      members.flatMap((member) => member.citations),
      citationKey,
    ),
    rests_on: unionBy(
      members.flatMap((member) => member.rests_on),
      anchorKey,
    ),
    plans: lead.plans,
  };
  MEMBERS.set(sentence, members);
  return sentence;
}

/**
 * Walks the sentences in order: one joins the current fragment when its keys meet the union of
 * the keys the fragment has gathered so far, else it starts a new one. A fragment of two or more
 * becomes one new sentence (see `merged`); a lone sentence is returned as it is.
 */
export function mergeFragments(sentences: Sentence[]): Sentence[] {
  const groups: Sentence[][] = [];
  let gathered = new Set<string>();
  for (const sentence of sentences) {
    const keys = openKeys(sentence);
    const current = groups.at(-1);
    if (current && keys.some((key) => gathered.has(key))) {
      current.push(sentence);
      for (const key of keys) gathered.add(key);
    } else {
      groups.push([sentence]);
      gathered = new Set(keys);
    }
  }
  return groups.map((group) => (group.length === 1 ? group[0]! : merged(group)));
}

/** The sentences a merged sentence stands for, in order; `[sentence]` for one that stands alone. */
export function fragmentMembers(sentence: Sentence): Sentence[] {
  return MEMBERS.get(sentence) ?? [sentence];
}

/** `page` with each paragraph's sentences merged; memoised per page object, `detail` untouched. */
export function columnPage(page: ConceptPage): ConceptPage {
  const known = VIEWS.get(page);
  if (known) return known;
  const view: ConceptPage = {
    ...page,
    blocks: page.blocks.map((block) =>
      block.kind === "paragraph" ? { ...block, sentences: mergeFragments(block.sentences) } : block,
    ),
  };
  VIEWS.set(page, view);
  return view;
}

export type RunEntry = { citation: SentenceCitation; sentence: Sentence | undefined };

type Resolved = { citation: SentenceCitation; sentence: Sentence };

/** Merges a stretch of resolved entries; a merged entry keeps its lead's citation. */
function mergedEntries(chunk: Resolved[]): RunEntry[] {
  const out: RunEntry[] = [];
  let at = 0;
  for (const sentence of mergeFragments(chunk.map((entry) => entry.sentence))) {
    out.push({ citation: chunk[at]!.citation, sentence });
    at += fragmentMembers(sentence).length;
  }
  return out;
}

/**
 * What an opened `citing` sentence shows, in citation order: each sentence citation resolved
 * through `lookup`, consecutive resolved entries merged by the merge rule; an unresolved entry
 * (its page is loading, rebuilt or gone) stands alone.
 */
export function runSentences(citing: Sentence, lookup: CitedLookup): RunEntry[] {
  const out: RunEntry[] = [];
  let chunk: Resolved[] = [];
  const flush = () => {
    out.push(...mergedEntries(chunk));
    chunk = [];
  };
  for (const citation of sentenceCitations(citing)) {
    const sentence = lookup(citation.summary, citation.id);
    if (sentence) {
      chunk.push({ citation, sentence });
    } else {
      flush();
      out.push({ citation, sentence });
    }
  }
  flush();
  return out;
}

/**
 * The open run (level below 2) of `sentence` at `parent`, matched by any member of the fragment:
 * a run opened before its sentence joined a fragment still belongs to the fragment.
 */
export function runOfSentence(
  open: OpenRun[],
  sentence: Sentence,
  parent: string | null,
): OpenRun | undefined {
  const ids = fragmentMembers(sentence).map((member) => member.id);
  return open.find((run) => ids.includes(run.sentenceId) && run.parent === parent && run.level < 2);
}

/** The open run at the site of any member of `site.sentence`, at `site.parent`. */
export function runAt(open: OpenRun[], site: SentenceSite): OpenRun | undefined {
  return runOfSentence(open, site.sentence, site.parent);
}
