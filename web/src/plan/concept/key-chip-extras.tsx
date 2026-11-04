import type { KeyChip } from "@/plan/api/schema-concept";
import { repoRel } from "@/plan/files/address";
import { useChangedSince } from "@/plan/review/changed-since";

/**
 * "changed" after a key chip whose document changed since the reader looked: a word in the fine
 * size, outside the chip's link, so the key's links stay one per document. The image role keeps
 * the name the file tree's marker uses ("changed since you looked").
 */
export function KeyChipExtras({ chip }: { chip: KeyChip }) {
  const changes = useChangedSince();
  if (!changes.some(({ path }) => path === repoRel(chip.doc.path))) return null;
  return (
    <span
      role="img"
      aria-label="changed since you looked"
      title="changed since you looked"
      className="plan-fine concept-key-changed"
    >
      <span aria-hidden="true">changed</span>
    </span>
  );
}
