import { Button } from "@/components/ui/button";
import { OTHER_GLYPHS } from "@/plan/ui/glyphs";

const SettingsIcon = OTHER_GLYPHS.settings;
const HelpIcon = OTHER_GLYPHS.help;

/** Icon plus visible label; the `aria-label` keeps the name stable for screen readers and tests. */
export function SettingsButton({ onClick }: { onClick: () => void }) {
  return (
    <Button variant="ghost" size="sm" className="plan-ibtn" aria-label="Settings" onClick={onClick}>
      <SettingsIcon aria-hidden="true" />
      <span>Settings</span>
    </Button>
  );
}

export function HelpButton({ onClick }: { onClick: () => void }) {
  return (
    <Button variant="ghost" size="sm" className="plan-ibtn" aria-label="Help" onClick={onClick}>
      <HelpIcon aria-hidden="true" />
      <span>Help</span>
    </Button>
  );
}
