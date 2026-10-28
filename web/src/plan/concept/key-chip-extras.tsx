import type { KeyChip } from "@/plan/api/schema-concept";
import { repoRel } from "@/plan/files/address";
import { useChangedSince } from "@/plan/review/changed-since";

export function KeyChipExtras({ chip }: { chip: KeyChip }) {
  const changes = useChangedSince();
  if (!changes.some(({ path }) => path === repoRel(chip.doc.path))) return null;
  return (
    <span
      role="img"
      aria-label="changed since you looked"
      title="changed since you looked"
      className="shrink-0 text-[12px] text-muted-foreground"
    >
      <span aria-hidden="true">●</span>
    </span>
  );
}
