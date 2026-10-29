import { useAtomValue } from "jotai/react";
import { Link, useLocation } from "react-router";

import { ThemeToggle } from "@/aurora-ui/theme/ThemeToggle";
import { Logo } from "@/components/logo";
import { Button } from "@/components/ui/button";
import { useMediaQuery } from "@/lib/use-media-query";
import { HeaderExtras } from "@/plan/header-extras";
import { ActivityPill } from "@/plan/header/activity";
import { FeatureSwitcher } from "@/plan/header/feature-switcher";
import { useResendStoredName } from "@/plan/header/name-prompt";
import { NextStepCard } from "@/plan/header/next-step";
import { Overflow } from "@/plan/header/overflow";
import { Presence } from "@/plan/header/presence";
import { Share } from "@/plan/header/share";
import { StepTracker } from "@/plan/header/step-tracker";
import { Tabs } from "@/plan/header/tabs";
import { checklistAtom } from "@/plan/state/atoms";
import { OTHER_GLYPHS } from "@/plan/ui/glyphs";

import "./header.css";

/** Below this the tracker collapses and presence and Share move to the overflow menu. */
const WIDE_QUERY = "(min-width: 1024px)";
const SettingsIcon = OTHER_GLYPHS.settings;
const HelpIcon = OTHER_GLYPHS.help;

type HeaderActions = { onOpenSettings: () => void; onOpenHelp: () => void };

function HeaderControls({
  narrow,
  sample,
  onOpenSettings,
  onOpenHelp,
}: HeaderActions & { narrow: boolean; sample: boolean }) {
  return (
    <div className="plan-header-controls header-controls ml-auto flex min-w-0 max-w-full shrink-0 items-center gap-2 overflow-x-auto [scrollbar-width:none]">
      <ActivityPill compact={narrow} />
      {!sample && <HeaderExtras compact={narrow} />}
      <Tabs />
      {narrow ? (
        <Overflow />
      ) : (
        <>
          <Presence />
          <Share />
        </>
      )}
      <ThemeToggle />
      <Button variant="ghost" size="icon" aria-label="Settings" onClick={onOpenSettings}>
        <SettingsIcon aria-hidden="true" />
      </Button>
      <Button variant="ghost" size="icon" aria-label="Help" onClick={onOpenHelp}>
        <HelpIcon aria-hidden="true" />
      </Button>
    </div>
  );
}

/**
 * `sample` leaves out the controls that fetch and subscribe to live state (run strip, summary
 * rebuilds, chat), for Help scenes that draw the header over seeded atoms.
 */
export function PlanHeader({
  onOpenSettings,
  onOpenHelp,
  sample = false,
}: HeaderActions & { sample?: boolean }) {
  const narrow = !useMediaQuery(WIDE_QUERY);
  const checklist = useAtomValue(checklistAtom);
  const { pathname } = useLocation();
  useResendStoredName();

  return (
    <header className="plan-header min-w-0 shrink-0 border-b border-hairline bg-card">
      <div className="mx-auto w-full max-w-[1920px] px-4 py-4 sm:px-6">
        <div className="grid grid-cols-[auto_minmax(4rem,1fr)_minmax(0,auto)] items-center gap-x-3 sm:gap-x-5 lg:gap-x-3">
          <Link to="/" className="shrink-0 text-(--logo)" aria-label="Loom planner home">
            <Logo className="h-5 w-auto" />
          </Link>
          <div className="min-w-0">
            <p className="eyebrow text-[10px]">Planning</p>
            <FeatureSwitcher />
          </div>
          <HeaderControls
            narrow={narrow}
            sample={sample}
            onOpenSettings={onOpenSettings}
            onOpenHelp={onOpenHelp}
          />
        </div>
        {pathname !== "/" && (
          <div className="mt-3 flex min-w-0 flex-nowrap items-center gap-3">
            <div className="flex min-w-0 flex-1 items-center">
              <StepTracker checklist={checklist} narrow={narrow} />
            </div>
            <div className="min-w-0 max-w-md shrink">
              <NextStepCard checklist={checklist} />
            </div>
          </div>
        )}
      </div>
    </header>
  );
}
