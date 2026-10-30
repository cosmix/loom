import { cn } from "cn";
import { Ellipsis } from "lucide-react";
import { useId, useState, type CSSProperties } from "react";
import { useAtomValue } from "jotai/react";
import { Link } from "react-router";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { docKindOfPageKey, pageHref, parsePageKey } from "@/plan/api/page-key";
import type { ConceptPage, ImplementsView, KeyChip } from "@/plan/api/schema-concept";
import { Untrusted } from "@/plan/chat/visible-controls";
import { DocMark } from "@/plan/ui/doc-mark";

import { KeyChipExtras } from "./key-chip-extras";
import { KeyRowExtras } from "./key-row-extras";
import { docColour } from "./marks";
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

/** A key chip: 28px high, the DocMark flush at the left padding. */
const CHIP_CLASS =
  "flex h-7 items-center gap-[7px] rounded-[8px] border border-hairline bg-card pr-2.5 pl-1 text-[12.5px] font-medium";

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
      className="concept-key-mark inline-flex shrink-0 rounded-[5px]"
      style={{ color: colour, "--key-colour": colour } as CSSProperties}
    >
      {/* Chip colours identify documents within a scope, independently of their kind. */}
      <DocMark kind={kind} rebuilding={rebuilding} className="!text-inherit !bg-transparent" />
    </span>
  );
}

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
  const link = (
    <Link
      to={pageHref(chip.doc.page)}
      data-dashed={chip.out_of_date ? "true" : undefined}
      aria-describedby={description ? descriptionId : undefined}
      className={cn(CHIP_CLASS, "data-[dashed=true]:border-dashed")}
    >
      <ChipMark chip={chip} rebuilding={rebuilding} />
      <span className="font-mono text-[11px] text-muted-foreground">
        <Untrusted text={chip.label} />
      </span>
      <span style={{ unicodeBidi: "isolate", direction: "ltr" }}>
        <SentenceText text={chipTitle(chip.doc.title)} />
      </span>
    </Link>
  );
  if (!description) return link;
  return (
    <Tooltip>
      <TooltipTrigger asChild>{link}</TooltipTrigger>
      <TooltipContent>
        <Untrusted text={description} />
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
    <li className="flex items-center gap-1.5">
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
      {chip.no_plan && <span className="text-xs text-muted-foreground">no plan</span>}
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
    <li className="flex items-center">
      <button
        type="button"
        aria-expanded={expanded}
        className={cn(
          CHIP_CLASS,
          "cursor-pointer px-2.5 text-muted-foreground hover:text-foreground",
        )}
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

/** The first four chips of the documents the page draws on, "more" for the rest, and the key row's extras. */
export function DocumentKey({ page, pageKey }: { page: ConceptPage; pageKey: string }) {
  // Expansion belongs to one page: another page opens collapsed without remounting the key row,
  // which would drop focus from the chip that navigated. Coming straight back finds it expanded.
  const [expandedOn, setExpandedOn] = useState<string | null>(null);
  const expanded = expandedOn === pageKey;
  const hidden = Math.max(0, page.key_chips.length - VISIBLE_CHIPS);
  const shown = expanded ? page.key_chips : page.key_chips.slice(0, VISIBLE_CHIPS);
  return (
    <div className="mt-1.5 mb-[30px] flex flex-col gap-2">
      <ul role="list" aria-label="Document key" className="flex flex-wrap items-center gap-1.5">
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
      <KeyRowExtras page={page} />
    </div>
  );
}
