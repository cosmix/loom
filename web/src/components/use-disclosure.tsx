import { ChevronRight } from "lucide-react";
import { useState } from "react";

/// Open state for a section that opens by default from the data (`auto`) until
/// the reader toggles it; from then on their choice holds through refreshes.
export function useDisclosure(auto: boolean): [open: boolean, toggle: () => void] {
  const [chosen, setChosen] = useState<boolean | null>(null);
  const open = chosen ?? auto;
  return [open, () => setChosen(!open)];
}

/// A section's eyebrow heading as the button that folds it: the chevron turns
/// down when open, `aria-expanded` and `aria-controls` name the panel.
export function DisclosureHeading({
  label,
  open,
  panel,
  onToggle,
}: {
  label: string;
  open: boolean;
  panel: string;
  onToggle: () => void;
}) {
  return (
    <h2 className="eyebrow">
      <button
        type="button"
        className="disclosure-toggle"
        aria-expanded={open}
        aria-controls={open ? panel : undefined}
        onClick={onToggle}
      >
        <ChevronRight aria-hidden="true" className="disclosure-chevron" />
        {label}
      </button>
    </h2>
  );
}
