import { useAtomValue } from "jotai";
import { Link } from "react-router";

import { repoRel, toAddress } from "@/plan/files/address";
import { OTHER_GLYPHS } from "@/plan/ui/glyphs";

import { openQuestionsAtom } from "./live";

const OpenQuestionsIcon = OTHER_GLYPHS.openQuestions;

/**
 * A tree row's open questions: the glyph and the count, linking to the document's list. The
 * words are read (`.sr-only`) and shown on hover; a document with none draws nothing.
 */
export function OpenQuestionCount({ path }: { path: string }) {
  const counts = useAtomValue(openQuestionsAtom);
  const count = counts[repoRel(path)];
  if (count === undefined || count === 0) return null;
  return (
    <Link
      to={toAddress(path, "open-questions")}
      title={`Open questions · ${count}`}
      className="plan-tree__questions"
    >
      <OpenQuestionsIcon data-other-glyph="openQuestions" aria-hidden="true" />
      <span className="sr-only">Open questions ·</span> {count}
    </Link>
  );
}
