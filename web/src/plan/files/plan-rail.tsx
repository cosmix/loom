import { useAtomValue } from "jotai/react";
import { Archive, Check, CircleAlert } from "lucide-react";
import { useId, type JSX } from "react";
import { Link } from "react-router";

import type { TreeEntry } from "@/plan/api/schema-files";
import { visibleControls } from "@/plan/chat/visible-controls";
import { SentenceText } from "@/plan/concept/sentence-text";
import { toAddress } from "@/plan/files/address";
import { AttentionBar, ProgressRing } from "@/plan/files/attention-bar";
import { openDocAtom } from "@/plan/files/atoms";
import {
  completeText,
  needsAttention,
  questionDocs,
  railNode,
  railSections,
  stageNames,
  type RailNode,
  type RailSection,
} from "@/plan/files/rail-model";
import {
  containsPath,
  gapAddress,
  plainTitle,
  TreeEntryRow,
  useDisclosure,
} from "@/plan/files/tree-entry";
import { usePlanErrors } from "@/plan/files/tree-entry-signals";
import { entryNumber, shortTitle } from "@/plan/files/tree-model";
import { openQuestionsAtom } from "@/plan/review/live";
import { DocMark } from "@/plan/ui/doc-mark";
import { OTHER_GLYPHS, PLAN_STATE_GLYPHS, type Glyph } from "@/plan/ui/glyphs";

const NODE_GLYPHS: Partial<Record<RailNode, Glyph>> = {
  done: Check,
  running: PLAN_STATE_GLYPHS.running,
  error: CircleAlert,
  question: OTHER_GLYPHS.openQuestions,
  retired: Archive,
};

const plural = (count: number, noun: string) => `${count} ${noun}${count === 1 ? "" : "s"}`;

/** The node's meaning in words: read after the title, and the start of the row's tooltip. */
function nodeWord(node: RailNode, errors: number, questions: number): string {
  if (node === "error") return plural(errors, "error");
  if (node === "question") return plural(questions, "open question");
  return node === "gap" ? "not written" : node;
}

/** A plan's place on the rail: a disc whose glyph says its state (the words are beside it). */
function Node({ node }: { node: RailNode }): JSX.Element {
  const Icon = NODE_GLYPHS[node];
  return (
    <span className="plan-rail__node" data-node={node} aria-hidden="true">
      {Icon && <Icon />}
    </span>
  );
}

/** The plan's stages under it once opened: the plan itself first, then its briefs as tree rows. */
function Stages({ plan, id, current }: { plan: TreeEntry; id: string; current: boolean }) {
  return (
    <ul id={id} className="plan-rail__stages plan-tree__list">
      {plan.path !== null && (
        <li className="plan-tree__item">
          <div className="plan-tree__row" data-current={current ? "" : undefined}>
            {/* An empty chevron column, so the plan lines up with the stage rows under it. */}
            <span className="plan-tree__twist" aria-hidden="true" />
            <Link
              to={toAddress(plan.path)}
              aria-current={current ? "page" : undefined}
              className="plan-tree__label"
              title={plan.file_name ? visibleControls(plan.file_name) : undefined}
            >
              <DocMark kind="plan" className="size-4 rounded-[4px]" />
              <span className="plan-tree__title">Plan document</span>
            </Link>
          </div>
        </li>
      )}
      {plan.children.map((child) => (
        <TreeEntryRow key={child.path ?? child.title} entry={child} forceOpen={false} />
      ))}
    </ul>
  );
}

/** What a rail row shows of a plan, from its entry, its live errors and its open questions. */
function useRailView(plan: TreeEntry, counts: Record<string, number>) {
  const errors = usePlanErrors(plan);
  const questions = questionDocs([plan], counts).reduce((sum, doc) => sum + doc.count, 0);
  const node = railNode(plan, errors, questions);
  const word = nodeWord(node, errors, questions);
  const sub = node === "gap" ? "not written" : stageNames(plan).join(" · ") || word;
  const tooltip = `${visibleControls(plainTitle(plan.title))} · ${word}\n${visibleControls(sub)}`;
  return { node, word, sub: visibleControls(sub), tooltip };
}

function RailBody({
  plan,
  node,
  word,
  sub,
}: {
  plan: TreeEntry;
  node: RailNode;
  word: string;
  sub: string;
}) {
  const number = entryNumber(plan);
  return (
    <>
      <Node node={node} />
      <span className="plan-rail__text">
        <span className="plan-rail__line">
          {number !== null && <span className="plan-rail__number">{number}</span>}{" "}
          <span className="plan-rail__title">
            <SentenceText text={shortTitle(plan)} />
          </span>
        </span>{" "}
        <span className="plan-rail__sub">{sub}</span>
        <span className="sr-only">, {word}</span>
      </span>
    </>
  );
}

/**
 * One plan on the rail: its node, mono number and short title over one muted line of stage
 * names. The row is a button that opens the plan's stages; a plan that needs the reader (running,
 * errors, open questions) or holds the open document starts open. A plan not written yet links
 * to its gap page instead.
 */
function RailItem({ plan, counts }: { plan: TreeEntry; counts: Record<string, number> }) {
  const open = useAtomValue(openDocAtom)?.doc.path ?? null;
  const { node, word, sub, tooltip } = useRailView(plan, counts);
  const holdsOpen = containsPath(plan, open);
  const [expanded, setExpanded] = useDisclosure(needsAttention(node), holdsOpen);
  const panel = useId();
  const body = <RailBody plan={plan} node={node} word={word} sub={sub} />;
  if (node === "gap") {
    return (
      <li className="plan-rail__entry">
        <Link
          to={gapAddress(plan.title, plan.feature, plan.target)}
          className="plan-rail__item"
          title={tooltip}
        >
          {body}
        </Link>
      </li>
    );
  }
  return (
    <li className="plan-rail__entry" data-current={holdsOpen ? "" : undefined}>
      <button
        type="button"
        className="plan-rail__item"
        aria-expanded={expanded}
        aria-controls={expanded ? panel : undefined}
        title={tooltip}
        onClick={() => setExpanded(!expanded)}
      >
        {body}
      </button>
      {expanded && <Stages plan={plan} id={panel} current={plan.path === open} />}
    </li>
  );
}

/** A section's head: its name (the metaplan's link, when it has one), its count and its ring. */
function RailHead({ section }: { section: RailSection }): JSX.Element {
  const open = useAtomValue(openDocAtom)?.doc.path ?? null;
  const { done, total, text } = completeText(section.plans);
  const { metaplan } = section;
  const name = <SentenceText text={section.name} />;
  const current = metaplan !== null && metaplan.path === open;
  return (
    <div className="plan-rail__head" data-current={current ? "" : undefined}>
      <div className="plan-rail__heading">
        {metaplan?.path ? (
          <Link
            to={toAddress(metaplan.path)}
            aria-current={current ? "page" : undefined}
            className="plan-rail__name"
            title={`Metaplan: ${visibleControls(plainTitle(metaplan.title))}`}
          >
            {name}
          </Link>
        ) : (
          <span className="plan-rail__name">{name}</span>
        )}
        <span className="plan-rail__complete">{text}</span>
      </div>
      <ProgressRing done={done} total={total} />
    </div>
  );
}

function RailSectionView({ section }: { section: RailSection }): JSX.Element {
  const counts = useAtomValue(openQuestionsAtom);
  const docs = section.metaplan ? [section.metaplan, ...section.plans] : section.plans;
  return (
    <section aria-label={visibleControls(plainTitle(section.name))} className="plan-rail">
      <RailHead section={section} />
      <AttentionBar docs={questionDocs(docs, counts)} />
      {section.plans.length > 0 && (
        <ol className="plan-rail__list">
          {section.plans.map((plan) => (
            <RailItem key={plan.path ?? plan.title} plan={plan} counts={counts} />
          ))}
        </ol>
      )}
    </section>
  );
}

/** The Plans tab: a rail per feature, then anything else the Plans category lists, as rows. */
export function PlansTab({ entries }: { entries: TreeEntry[] }): JSX.Element {
  const { sections, rest } = railSections(entries);
  if (sections.length === 0 && rest.length === 0) {
    return <p className="plan-empty plan-rail__empty">No plans yet.</p>;
  }
  return (
    <div className="plan-rail-tab">
      {sections.map((section) => (
        <RailSectionView key={section.key} section={section} />
      ))}
      {rest.length > 0 && (
        <ul className="plan-tree__list">
          {rest.map((entry) => (
            <TreeEntryRow key={entry.path ?? entry.title} entry={entry} forceOpen={false} />
          ))}
        </ul>
      )}
    </div>
  );
}
