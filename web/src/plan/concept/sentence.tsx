import { cn } from "cn";
import { ChevronDown } from "lucide-react";
import { Fragment, type CSSProperties, type JSX } from "react";

import type { ConceptPage, PlanLink, SectionLink, Sentence } from "@/plan/api/schema-concept";

import { canOpen, runDomId, sentenceKey, useColumn, type SentenceSite } from "./column-context";
import { FLASH_CLASS, flashKey } from "./flash";
import { fragmentMembers } from "./fragments";
import { docColour, Marks } from "./marks";
import { PlanGlyph } from "./plan-glyph";
import { SentenceText } from "./sentence-text";

/** A click that ends a text selection selects; it opens nothing (selection is for chat). */
export function endsSelection(): boolean {
  const selection = window.getSelection();
  return selection !== null && !selection.isCollapsed && selection.toString().length > 0;
}

type ClosedSentenceProps = {
  site: SentenceSite;
  /** Show the plan glyph: this is the first sentence of a drilled-into document. */
  glyph: boolean;
  /** The page key whose arrival flashes this sentence. */
  flashPage: string;
};

/** What follows the text: the marks and a chevron hint, or the muted "no source" mark. */
function SentenceTail({ site, button }: { site: SentenceSite; button: boolean }): JSX.Element {
  const column = useColumn();
  if (!button) return <span className="concept-no-source">no source</span>;
  return (
    <>
      <Marks marks={site.marks} onOpen={column.openPage} />
      {/* The anchor takes no inline width; the chevron floats just after the marks. */}
      <span aria-hidden="true" className="concept-sentence-hint">
        <ChevronDown className="concept-sentence-chevron" />
      </span>
    </>
  );
}

/**
 * The text of a fragment of two or more sentences: one span per member, so a selection inside
 * it keeps each sentence's provenance (`selection.ts` reads the closest `data-sentence-id`).
 */
function FragmentText({ members, summary }: { members: Sentence[]; summary: string }): JSX.Element {
  return (
    <>
      {members.map((member, index) => (
        <Fragment key={member.id}>
          {index > 0 && " "}
          <span data-sentence-id={member.id} data-summary={summary}>
            <SentenceText text={member.text} />
          </span>
        </Fragment>
      ))}
    </>
  );
}

/**
 * A summary sentence that is not open. With citations it is a button that opens in place; with
 * none it is a defect (section 05) and shows a muted "no source" mark instead of colour marks.
 * Marks, the chevron hint and the plan glyph sit after the inline text, outside the button.
 * A fragment (several sentences that open to the same content) is one button with one tail;
 * its wrap carries `data-fragment` with the member count, which a single sentence's does not.
 */
export function ClosedSentence({ site, glyph, flashPage }: ClosedSentenceProps): JSX.Element {
  const column = useColumn();
  const { sentence, summary, parent } = site;
  const members = fragmentMembers(sentence);
  const button = canOpen(sentence);
  const flashing = members.some((member) => column.flashing.has(flashKey(flashPage, member.id)));
  const cited = glyph ? column.citedPage(summary) : null;
  return (
    <span
      className="concept-sentence-wrap"
      data-fragment={members.length > 1 ? members.length : undefined}
      style={{ "--sentence-colour": docColour(site.marks[0]?.colour ?? 0) } as CSSProperties}
    >
      <span
        role={button ? "button" : undefined}
        aria-expanded={button ? false : undefined}
        aria-controls={button ? runDomId(parent, sentence.id) : undefined}
        tabIndex={-1}
        // Isolated like the other untrusted text, so a format character cannot reorder the
        // sentences around it; the element is also the text parent the selection code reads.
        style={{ unicodeBidi: "isolate", direction: "ltr" }}
        data-focus={sentenceKey(parent, sentence.id)}
        data-sentence-id={sentence.id}
        data-summary={summary}
        className={cn(
          "concept-sentence",
          !button && "concept-sentence-plain",
          flashing && FLASH_CLASS,
        )}
        onClick={
          button
            ? () => {
                if (!endsSelection()) column.openSentence(site);
              }
            : undefined
        }
      >
        {members.length > 1 ? (
          <FragmentText members={members} summary={summary} />
        ) : (
          <SentenceText text={sentence.text} />
        )}
      </span>
      <SentenceTail site={site} button={button} />
      {cited && <PlanGlyph plans={plansOf(cited, sentence)} sections={implementsOf(cited)} />}
    </span>
  );
}

/**
 * The plans implementing the drilled-into product section. The server sets `plans` only on
 * the first sentence of each paragraph, and a run may start at any sentence, so they are read
 * from the first sentence of the paragraph holding `sentence` on the cited page.
 * Not from the page's `implemented_by`: the frozen contract "concept opening an overview
 * sentence shows its cited summary sentences in place" pins a run without a glyph on
 * fixtures whose pages list implementing plans and whose sentences carry none.
 */
function plansOf(page: ConceptPage, sentence: Sentence): PlanLink[] {
  const paragraph = page.blocks.find(
    (block) => block.kind === "paragraph" && block.sentences.some((s) => s.id === sentence.id),
  );
  const first = paragraph?.kind === "paragraph" ? paragraph.sentences[0] : undefined;
  return first?.plans ?? sentence.plans;
}

function implementsOf(page: ConceptPage): SectionLink[] {
  return page.implements?.kind === "sections" ? page.implements.links : [];
}
