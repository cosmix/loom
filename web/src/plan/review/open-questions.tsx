import { useAtomValue } from "jotai";
import { Link } from "react-router";

import { repoRel, toAddress } from "@/plan/files/address";
import { OTHER_GLYPHS } from "@/plan/ui/glyphs";

import { openQuestionsAtom } from "./live";

const OpenQuestionsIcon = OTHER_GLYPHS.openQuestions;

export function OpenQuestionCount({ path }: { path: string }) {
  const counts = useAtomValue(openQuestionsAtom);
  const count = counts[repoRel(path)];
  if (count === undefined) return null;
  return (
    <Link
      to={toAddress(path, "open-questions")}
      className="inline-flex items-center gap-1 text-[13px] font-medium underline"
    >
      <OpenQuestionsIcon
        data-other-glyph="openQuestions"
        className="size-4 shrink-0"
        aria-hidden="true"
      />
      Open questions · {count}
    </Link>
  );
}
