import { X } from "lucide-react";
import type { JSX } from "react";

/** The remove control every attachment row ends with: one size, one hit area, the row's name. */
export function ChipRemove({
  label,
  onRemove,
}: {
  label: string;
  onRemove: () => void;
}): JSX.Element {
  return (
    <button
      type="button"
      className="chat-chip-remove"
      aria-label={`Remove ${label}`}
      onClick={onRemove}
    >
      <X size={14} aria-hidden="true" />
    </button>
  );
}
