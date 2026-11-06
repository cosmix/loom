import { Ellipsis } from "lucide-react";
import { useId, useState, type CSSProperties } from "react";
import { useAtomValue } from "jotai/react";
import { Link } from "react-router";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { docKindOfPageKey, isOverviewKey, pageHref, parsePageKey } from "@/plan/api/page-key";
import type { ConceptPage, ImplementsView, KeyChip } from "@/plan/api/schema-concept";
import { Untrusted } from "@/plan/chat/visible-controls";
import { DocMark } from "@/plan/ui/doc-mark";

import "./concept-page.css";
import { KeyChipExtras } from "./key-chip-extras";
import { docColour } from "./marks";
import { pageSentences } from "./open-state";
import { conceptPageAtom } from "./page-state";
import { SentenceText } from "./sentence-text";
import { outOfDateText } from "./summary-text";

function implementsText(view: ImplementsView | null): string | null {
  if (!view) return null;
  if (view.kind === "not_stated") return "Implements: not stated";
  return `Implements: ${view.links.map((link) => link.doc?.title ?? link.name).join(", ")}`;
}

/** The chip's tooltip: why it is out of date, and for a plan chip what it implements. */
function chipDescription(chip: KeyChip, implementsView: ImplementsView | null): string {
  const lines = [chip.out_of_date ? outOfDateText(chip.out_of_date) : null];
  const key = parsePageKey(chip.doc.page);
  if (key?.kind === "standalone" || key?.scope === "engineering") {
    lines.push(implementsText(implementsView));
  }
  return lines.filter((line) => line !== null).join(". ");
}

export function chipTitle(title: string): string {
  // Spaces around the dash are required, so "3-D rendering" keeps its title.
  return title.replace(/^\s*\d+\s+[—–-]\s+/, "");
}

/** The key shows this many chips; the rest sit behind the "more" chip. */
const VISIBLE_CHIPS = 4;

function ChipMark({ chip, rebuilding }: { chip: KeyChip; rebuilding: boolean }) {
  const kind = docKindOfPageKey(chip.doc.page);
  if (kind === null) return null;
  const colour = docColour(chip.colour);
  return (
    <span
      aria-hidden="true"
      data-kind={kind}
      data-rebuilding={rebuilding}
      className="concept-key-mark"
      style={{ color: colour, "--key-colour": colour } as CSSProperties}
    >
      {/* Chip colours identify documents within a scope, independently of their kind. */}
      <DocMark kind={kind} rebuilding={rebuilding} className="!text-inherit !bg-transparent" />
    </span>
  );
}

/** A section reads by its number ("§05"); any other document by its label as sent (a plan slug). */
function shortLabel(chip: KeyChip): string {
  const section = docKindOfPageKey(chip.doc.page) === "section" && /^\d+$/.test(chip.label);
  return section ? `§${chip.label}` : chip.label;
}

/**
 * A document in the key: its mark in the document colour, its short label and its title. The
 * title is drawn whole in the DOM and cut to the chip's width by CSS alone (`.concept-key-title`),
 * so the link's accessible name keeps all of it; the tooltip repeats it in full, above the
 * out-of-date or implements note when there is one.
 */
function ChipLink({
  chip,
  rebuilding,
  description,
  descriptionId,
}: {
  chip: KeyChip;
  rebuilding: boolean;
  description: string;
  descriptionId: string;
}) {
  const title = chipTitle(chip.doc.title);
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Link
          to={pageHref(chip.doc.page)}
          data-dashed={chip.out_of_date ? "true" : undefined}
          aria-describedby={description ? descriptionId : undefined}
          className="concept-key-doc"
        >
          <ChipMark chip={chip} rebuilding={rebuilding} />
          <span className="concept-key-label font-mono">
            <Untrusted text={shortLabel(chip)} />
          </span>{" "}
          <span className="concept-key-title" style={{ unicodeBidi: "isolate", direction: "ltr" }}>
            <SentenceText text={title} />
          </span>
        </Link>
      </TooltipTrigger>
      <TooltipContent className="max-w-[48ch] flex-col items-start">
        <SentenceText text={title} />
        {description && (
          <span className="block opacity-80">
            <Untrusted text={description} />
          </span>
        )}
      </TooltipContent>
    </Tooltip>
  );
}

function Chip({ chip }: { chip: KeyChip }) {
  const descriptionId = useId();
  const loaded = useAtomValue(conceptPageAtom(chip.doc.page)).page;
  const description = chipDescription(chip, loaded?.implements ?? null);
  const rebuilding =
    chip.out_of_date?.reason === "rebuilding" ||
    loaded?.state?.state === "rebuilding" ||
    loaded?.state?.out_of_date?.reason === "rebuilding";
  return (
    <li>
      <ChipLink
        chip={chip}
        rebuilding={rebuilding}
        description={description}
        descriptionId={descriptionId}
      />
      {description && (
        <span id={descriptionId} className="sr-only">
          <Untrusted text={description} />
        </span>
      )}
      {chip.no_plan && <span className="plan-fine text-muted-foreground">no plan</span>}
      <KeyChipExtras chip={chip} />
    </li>
  );
}

/** The last chip of a long key: reveals the hidden chips in place, then folds them away. */
function MoreChip({
  hidden,
  expanded,
  onToggle,
}: {
  hidden: number;
  expanded: boolean;
  onToggle: () => void;
}) {
  return (
    <li>
      <button
        type="button"
        aria-expanded={expanded}
        className="concept-key-doc concept-key-more"
        onClick={onToggle}
      >
        {expanded ? (
          "Fewer"
        ) : (
          <>
            <Ellipsis aria-hidden="true" className="size-3.5" />
            {/* The ellipsis is a glyph for sighted readers; the name spells it out. */}
            <span className="sr-only">…</span>
            {`${hidden} more`}
          </>
        )}
      </button>
    </li>
  );
}

/** What the key's documents are, by kind: all sections, all plans, or a mix of documents. */
function sourceNoun(chips: readonly KeyChip[]): string {
  const kinds = new Set(chips.map((chip) => docKindOfPageKey(chip.doc.page)));
  const only = kinds.size === 1 ? [...kinds][0] : null;
  const noun = only === "section" || only === "plan" ? only : "document";
  return chips.length === 1 ? noun : `${chips.length} ${noun}s`;
}

/**
 * The key's one-line lede. An overview summarizes the key's documents; any other page summarizes
 * its own document, so there the key is only the colour legend of the feature.
 */
export function keyLede(page: ConceptPage): string {
  const chips = page.key_chips;
  if (!isOverviewKey(page.key)) return `Key: the ${sourceNoun(chips)} of this feature`;
  return `${chips.length === 1 ? "Source" : "Sources"}: the ${sourceNoun(chips)} this overview summarizes`;
}

/** Whether any sentence on the page carries a mark, so the mark legend has something to explain. */
function hasMarks(page: ConceptPage): boolean {
  return pageSentences(page).some((sentence) => sentence.marks.length > 0);
}

/**
 * What the spine cannot show: which colour is which document, and what the squares after a
 * sentence mean. On the page ground: one head line holding the lede and, when the page has
 * marks, the mark legend at its end; then per document its mark, number and title on a
 * fixed-width track, the first four with "more" for the rest. No chips, no box (DESIGN.md,
 * Surfaces).
 */
export function DocumentKey({ page, pageKey }: { page: ConceptPage; pageKey: string }) {
  // Expansion belongs to one page: another page opens collapsed without remounting the key row,
  // which would drop focus from the chip that navigated. Coming straight back finds it expanded.
  const [expandedOn, setExpandedOn] = useState<string | null>(null);
  const ledeId = useId();
  const expanded = expandedOn === pageKey;
  const hidden = Math.max(0, page.key_chips.length - VISIBLE_CHIPS);
  const shown = expanded ? page.key_chips : page.key_chips.slice(0, VISIBLE_CHIPS);
  const dashed = page.key_chips.some((chip) => chip.out_of_date !== null);
  return (
    <div className="concept-key">
      <div className="concept-key-head">
        <p id={ledeId} className="plan-fine concept-key-lede">
          {keyLede(page)}
        </p>
        {hasMarks(page) && (
          <p className="plan-fine concept-key-legend">
            <span aria-hidden="true" className="concept-key-square" />
            {dashed
              ? "A square after a sentence names a document it draws on. Dashed means out of date."
              : "A square after a sentence names a document it draws on."}
          </p>
        )}
      </div>
      <ul
        role="list"
        aria-label="Document key"
        aria-describedby={ledeId}
        className="concept-key-list"
      >
        {shown.map((chip) => (
          <Chip key={chip.doc.id} chip={chip} />
        ))}
        {hidden > 0 && (
          <MoreChip
            hidden={hidden}
            expanded={expanded}
            onToggle={() => setExpandedOn(expanded ? null : pageKey)}
          />
        )}
      </ul>
    </div>
  );
}
