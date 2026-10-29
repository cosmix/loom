import { Fragment, type JSX } from "react";

import { isOverviewKey } from "@/plan/api/page-key";
import type { Block, Sentence } from "@/plan/api/schema-concept";

import { summaryIdOfKey, useColumn } from "./column-context";
import { FoldNoteLine } from "./fold-note";
import { pageMark } from "./marks";
import { SentenceNode } from "./run";

/**
 * A paragraph is a block that holds inline sentences and, where a sentence opened to
 * passages, the passage blocks, so the text before and after them stays in one paragraph.
 * Fold notes follow the sentence whose run they replaced (`paragraphOf`), or lead the page.
 */
function Paragraph({ sentences, first }: { sentences: Sentence[]; first: boolean }): JSX.Element {
  const column = useColumn();
  const summary = summaryIdOfKey(column.pageKey);
  // A document summary page draws its own document's mark when its sentences carry none.
  const own = !isOverviewKey(column.pageKey)
    ? pageMark(column.page, column.pageKey, summary)
    : null;
  const notesAfter = (id: string) => column.notes.filter((note) => note.paragraphOf === id);
  return (
    <div className="concept-paragraph">
      {first &&
        notesAfter("").map((note, i) => <FoldNoteLine key={`${i}|${note.text}`} note={note} />)}
      {sentences.map((sentence, index) => (
        <Fragment key={sentence.id}>
          {index > 0 && " "}
          <SentenceNode
            site={{
              sentence,
              summary,
              parent: null,
              marks: sentence.marks.length > 0 || !own ? sentence.marks : [own],
            }}
            glyph={false}
            flashPage={column.pageKey}
          />
          {notesAfter(sentence.id).map((note, i) => (
            <FoldNoteLine key={`${i}|${note.text}`} note={note} />
          ))}
        </Fragment>
      ))}
    </div>
  );
}

function Heading({ level, text }: { level: number; text: string }): JSX.Element {
  const Tag = `h${Math.min(level + 1, 6)}` as "h2";
  return <Tag className={`concept-heading concept-h${Math.min(level, 3)}`}>{text}</Tag>;
}

/** The page's headings and paragraphs, in the order section 04 produced them. */
export function Blocks({ blocks }: { blocks: Block[] }): JSX.Element {
  const firstParagraph = blocks.findIndex((block) => block.kind === "paragraph");
  return (
    <>
      {blocks.map((block, index) =>
        block.kind === "heading" ? (
          <Heading key={index} level={block.level} text={block.text} />
        ) : (
          <Paragraph key={index} sentences={block.sentences} first={index === firstParagraph} />
        ),
      )}
    </>
  );
}
