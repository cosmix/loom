import { useId } from "react";
import { useAtomValue, useSetAtom } from "jotai/react";
import { Link } from "react-router";

import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { docKindOfPageKey, pageHref, parsePageKey } from "@/plan/api/page-key";
import type { ConceptPage, ImplementsView, KeyChip } from "@/plan/api/schema-concept";
import { DocMark } from "@/plan/ui/doc-mark";

import { KeyChipExtras } from "./key-chip-extras";
import { KeyRowExtras } from "./key-row-extras";
import { docColour } from "./marks";
import { conceptPageAtom } from "./page-state";
import { SentenceText } from "./sentence-text";
import { outOfDateText } from "./summary-text";
import { openRunsAtom } from "./view-state";

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

function ChipMark({ chip, rebuilding }: { chip: KeyChip; rebuilding: boolean }) {
  const kind = docKindOfPageKey(chip.doc.page);
  if (kind === null) return null;
  const colour = docColour(chip.colour);
  return (
    <span
      aria-hidden="true"
      data-kind={kind}
      data-rebuilding={rebuilding}
      className="inline-flex shrink-0 rounded"
      style={{ color: colour, backgroundColor: `color-mix(in oklch, ${colour} 14%, transparent)` }}
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
      className="flex items-center gap-1.5 rounded-md border border-hairline px-2 py-1 text-[13px] font-medium data-[dashed=true]:border-dashed"
    >
      <ChipMark chip={chip} rebuilding={rebuilding} />
      <span className="font-mono text-xs">{chip.label}</span>
      <span>
        <SentenceText text={chipTitle(chip.doc.title)} />
      </span>
    </Link>
  );
  if (!description) return link;
  return (
    <Tooltip>
      <TooltipTrigger asChild>{link}</TooltipTrigger>
      <TooltipContent>{description}</TooltipContent>
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
          {description}
        </span>
      )}
      {chip.no_plan && <span className="text-xs text-muted-foreground">no plan</span>}
      <KeyChipExtras chip={chip} />
    </li>
  );
}

/** One chip per document the page draws on, a Fold all button, and the key row's extras. */
export function DocumentKey({ page, pageKey }: { page: ConceptPage; pageKey: string }) {
  const setOpen = useSetAtom(openRunsAtom(pageKey));
  return (
    <div className="flex flex-col gap-2 pb-4">
      <div className="flex flex-wrap items-center gap-2">
        <ul role="list" aria-label="Document key" className="flex flex-wrap items-center gap-2">
          {page.key_chips.map((chip) => (
            <Chip key={chip.doc.id} chip={chip} />
          ))}
        </ul>
        <Button type="button" variant="ghost" size="sm" onClick={() => setOpen([])}>
          Fold all
        </Button>
      </div>
      <KeyRowExtras page={page} />
    </div>
  );
}
