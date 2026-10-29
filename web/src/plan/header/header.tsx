import { useAtomValue } from "jotai/react";
import { Link, useLocation, useMatch } from "react-router";

import { ThemeToggle } from "@/aurora-ui/theme/ThemeToggle";
import { Logo } from "@/components/logo";
import { Button } from "@/components/ui/button";
import { useMediaQuery } from "@/lib/use-media-query";
import { featureOfKey, pageKeyOfPath } from "@/plan/api/page-key";
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
import { featuresAtom } from "@/plan/state/atoms";
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
    <div className="plan-header-controls header-controls col-span-2 flex min-w-0 max-w-full flex-wrap items-center gap-2 sm:col-span-1 sm:ml-auto sm:justify-end">
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

function FeatureProgress({ narrow }: { narrow: boolean }) {
  const { pathname } = useLocation();
  const featureRoute = useMatch("/f/:feature");
  const features = useAtomValue(featuresAtom);
  const pageKey = pageKeyOfPath(pathname);
  const feature =
    (pageKey === null ? null : featureOfKey(pageKey)) ?? featureRoute?.params.feature ?? null;
  const checklist = features.find((candidate) => candidate.feature === feature) ?? null;
  if (checklist === null) return null;

  return (
    <div className="mt-3 grid min-w-0 grid-cols-[auto_minmax(0,1fr)] items-center gap-3 lg:flex">
      <div className="flex min-w-0 items-center lg:flex-1">
        <StepTracker checklist={checklist} narrow={narrow} />
      </div>
      <div className="min-w-0 max-w-md shrink">
        <NextStepCard checklist={checklist} />
      </div>
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
  useResendStoredName();

  return (
    <header className="plan-header min-w-0 shrink-0 border-b border-hairline bg-card">
      <div className="mx-auto w-full max-w-[1920px] px-4 py-4 sm:px-6">
        <div className="grid grid-cols-[auto_minmax(8rem,1fr)] items-center gap-x-3 gap-y-3 sm:grid-cols-[auto_minmax(4rem,1fr)_minmax(0,auto)] sm:gap-x-5 lg:gap-x-3">
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
        <FeatureProgress narrow={narrow} />
      </div>
    </header>
  );
}
