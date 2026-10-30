import { useAtomValue } from "jotai/react";
import { Link, useLocation, useMatch } from "react-router";

import { ThemeToggle } from "@/aurora-ui/theme/ThemeToggle";
import { Logo } from "@/components/logo";
import { useMediaQuery } from "@/lib/use-media-query";
import { featureOfKey, pageKeyOfPath } from "@/plan/api/page-key";
import { hasChatPanel } from "@/plan/dock";
import { HeaderExtras } from "@/plan/header-extras";
import { ActivityPill } from "@/plan/header/activity";
import { Identity } from "@/plan/header/feature-switcher";
import { HelpButton, SettingsButton } from "@/plan/header/header-buttons";
import { useResendStoredName } from "@/plan/header/name-prompt";
import { NextStepCard } from "@/plan/header/next-step";
import { Overflow } from "@/plan/header/overflow";
import { PressureStrip } from "@/plan/header/pressure-strip";
import { Presence } from "@/plan/header/presence";
import { Share } from "@/plan/header/share";
import { StepTracker } from "@/plan/header/step-tracker";
import { Tabs } from "@/plan/header/tabs";
import { featuresAtom } from "@/plan/state/atoms";

import "./header.css";

/** Below this the tracker collapses and everything but the activity pill moves to the overflow menu. */
const WIDE_QUERY = "(min-width: 1024px)";

type HeaderActions = { onOpenSettings: () => void; onOpenHelp: () => void };

function HeaderControls({
  narrow,
  sample,
  onOpenSettings,
  onOpenHelp,
}: HeaderActions & { narrow: boolean; sample: boolean }) {
  const { pathname } = useLocation();
  return (
    <div className="plan-header-controls header-controls">
      <ActivityPill compact={narrow} sample={sample} />
      {/* The chat button only means something where the dock mounts the panel. */}
      {!sample && <HeaderExtras compact={narrow && hasChatPanel(pathname)} />}
      {narrow ? (
        <Overflow onOpenSettings={onOpenSettings} onOpenHelp={onOpenHelp} />
      ) : (
        <>
          <Tabs />
          <Presence />
          <Share />
          <ThemeToggle />
          <SettingsButton onClick={onOpenSettings} />
          <HelpButton onClick={onOpenHelp} />
        </>
      )}
    </div>
  );
}

/** The second row of a feature route: the step rail and, beside it, the next-step card. */
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
    <div className="plan-header-rail" data-narrow={narrow ? "" : undefined}>
      <StepTracker checklist={checklist} narrow={narrow} />
      <NextStepCard checklist={checklist} compact={narrow} />
    </div>
  );
}

/**
 * `sample` leaves out the controls that fetch and subscribe to live state (activity rows, summary
 * rebuilds, chat, the pressure strip), for Help scenes that draw the header over seeded atoms.
 */
export function PlanHeader({
  onOpenSettings,
  onOpenHelp,
  sample = false,
}: HeaderActions & { sample?: boolean }) {
  const narrow = !useMediaQuery(WIDE_QUERY);
  useResendStoredName();

  return (
    <header className="plan-header">
      <div className="mx-auto w-full max-w-[1920px]">
        <div className="plan-appbar">
          <Link to="/" className="plan-logo-link" aria-label="Loom planner home">
            <Logo className="plan-logo" />
          </Link>
          <Identity />
          <HeaderControls
            narrow={narrow}
            sample={sample}
            onOpenSettings={onOpenSettings}
            onOpenHelp={onOpenHelp}
          />
          <FeatureProgress narrow={narrow} />
        </div>
      </div>
      {!sample && <PressureStrip />}
    </header>
  );
}
