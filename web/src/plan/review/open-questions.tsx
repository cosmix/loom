import { useAtomValue } from "jotai";
import { Link } from "react-router";

import { repoRel, toAddress } from "@/plan/files/address";

import { openQuestionsAtom } from "./live";

export function OpenQuestionCount({ path }: { path: string }) {
  const counts = useAtomValue(openQuestionsAtom);
  const count = counts[repoRel(path)];
  if (count === undefined) return null;
  return (
    <Link to={toAddress(path, "open-questions")} className="text-[13px] font-medium underline">
      Open questions · {count}
    </Link>
  );
}
