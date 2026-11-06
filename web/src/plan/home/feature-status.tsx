import { Gauge } from "lucide-react";
import type { ReactNode } from "react";

import type { Attention } from "@/plan/home/attention";
import { ArrowSpinner } from "@/plan/ui/spinner";
import { DOC_KIND_GLYPHS, OTHER_GLYPHS } from "@/plan/ui/glyphs";
import { plural } from "@/plan/ui/plural";

const QuestionGlyph = OTHER_GLYPHS.openQuestions;
const FindingGlyph = DOC_KIND_GLYPHS.review;

function Item({ kind, glyph, children }: { kind: string; glyph: ReactNode; children: string }) {
  return (
    <li data-kind={kind} className="plan-status feature-status__item">
      {glyph}
      {children}
    </li>
  );
}

/**
 * What asks for the reader, most urgent first: plans running now, open questions in the spec,
 * findings the last run left open, plans with no pressure test. Each is a glyph and a word; a
 * feature with none of them draws nothing here.
 */
export function FeatureStatus({ id, attention }: { id: string; attention: Attention }) {
  const { running, questions, findings, untested } = attention;
  if (running + questions + findings + untested === 0) return null;

  return (
    <ul id={id} aria-label="Needs attention" className="feature-status">
      {running > 0 && (
        <Item kind="running" glyph={<ArrowSpinner className="tone-executing" />}>
          {`${plural(running, "plan")} running`}
        </Item>
      )}
      {questions > 0 && (
        <Item
          kind="questions"
          glyph={<QuestionGlyph aria-hidden="true" className="tone-warning" />}
        >
          {plural(questions, "open question")}
        </Item>
      )}
      {findings > 0 && (
        <Item kind="findings" glyph={<FindingGlyph aria-hidden="true" className="tone-warning" />}>
          {plural(findings, "open finding")}
        </Item>
      )}
      {untested > 0 && (
        <Item
          kind="untested"
          glyph={<Gauge aria-hidden="true" className="feature-status__quiet" />}
        >
          {`${untested} not pressure-tested`}
        </Item>
      )}
    </ul>
  );
}
