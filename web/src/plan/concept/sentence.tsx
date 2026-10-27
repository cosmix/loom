import { cn } from "cn";
import type { JSX } from "react";

import type { ConceptPage, PlanLink, SectionLink, Sentence } from "@/plan/api/schema-concept";

import { canOpen, runDomId, sentenceKey, useColumn, type SentenceSite } from "./column-context";
import { FLASH_CLASS, flashKey } from "./flash";
import { Marks } from "./marks";
import { PlanGlyph } from "./plan-glyph";

/** A click that ends a text selection selects; it opens nothing (selection is for chat). */
export function endsSelection(): boolean {
  const selection = window.getSelection();
  return selection !== null && !selection.isCollapsed && selection.toString().length > 0;
}

/**
 * A summary sentence that is not open. With citations it is a button that opens in place; with
 * none it is a defect (section 05) and shows a muted "no source" mark instead of colour marks.
 * The text is one text node; marks and the plan glyph sit after it, outside the button.
 */
export function ClosedSentence({
  site,
  glyph,
  flashPage,
}: {
  site: SentenceSite;
  /** Show the plan glyph: this is the first sentence of a drilled-into document. */
  glyph: boolean;
  /** The page key whose arrival flashes this sentence. */
  flashPage: string;
}): JSX.Element {
  const column = useColumn();
  const { sentence, summary, parent } = site;
  const button = canOpen(sentence);
  const flashing = column.flashing.has(flashKey(flashPage, sentence.id));
  const cited = glyph ? column.citedPage(summary) : null;
  return (
    <span>
      <span
        role={button ? "button" : undefined}
        aria-expanded={button ? false : undefined}
        aria-controls={button ? runDomId(parent, sentence.id) : undefined}
        tabIndex={-1}
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
        {sentence.text}
      </span>
      {button ? (
        <Marks marks={site.marks} onOpen={column.openPage} />
      ) : (
        <span className="concept-no-source">no source</span>
      )}
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
