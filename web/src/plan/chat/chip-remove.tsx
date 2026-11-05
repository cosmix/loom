import { X } from "lucide-react";
import type { JSX } from "react";

/** The remove control every attachment row ends with: one size, one hit area, the row's name. */
export function ChipRemove({
  label,
  verb = "Remove",
  onRemove,
}: {
  label: string;
  /** The action word of the accessible name; a context line is dismissed, not removed. */
  verb?: string;
  onRemove: () => void;
}): JSX.Element {
  return (
    <button
      type="button"
      className="chat-chip-remove"
      aria-label={`${verb} ${label}`}
      onClick={onRemove}
    >
      <X size={14} aria-hidden="true" />
    </button>
  );
}
