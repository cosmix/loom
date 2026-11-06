import { useAtomValue, useStore } from "jotai/react";
import { useEffect, useState } from "react";

import type { ChecklistView } from "@/plan/api/schema";
import { ConfigBlockNotice } from "@/plan/chat/pressure/config-block-notice";
import { Untrusted } from "@/plan/chat/visible-controls";
import { NoPlans, PlanPage, UnwrittenPlans } from "@/plan/feature/plan-page";
import { PlanName, PlanStateStatus } from "@/plan/feature/plan-state";
import { runScope, runSentence, startControl, type RunRead } from "@/plan/feature/run-scope";
import { useFeature } from "@/plan/feature/use-feature";
import { DashboardLink, webUrl } from "@/plan/run/dashboard-button";
import { StartRunButton } from "@/plan/run/plan-run-actions";
import { fetchRunState, runStateAtom, runStripAtom } from "@/plan/run/use-run";
import { LoadState } from "@/plan/ui/load-state";

function PlanRows({ checklist }: { checklist: ChecklistView }) {
  return (
    <ul aria-label="Plans" className="plan-list plan-page__rows">
      {checklist.plan_rows.map((row) => (
        <li key={row.path}>
          <PlanName feature={checklist.feature} slug={row.slug} />
          <span className="plan-page__side">
            <PlanStateStatus state={row.state} />
          </span>
        </li>
      ))}
    </ul>
  );
}

/**
 * The page's one Start a run button (`startControl`), with the Codex configuration notice under
 * it, and a dashboard link while a run of this feature (or none) is up. It is the screen's
 * primary only when the checklist's primary is the start_run offer.
 */
function RunActions({ checklist, read }: { checklist: ChecklistView; read: RunRead }) {
  const state = useAtomValue(runStateAtom);
  const control = startControl(checklist, state, read);
  const dashboard = runScope(checklist, state) === "foreign" ? null : webUrl(state?.dashboard_url);
  if (control === null && dashboard === null) return null;
  return (
    <div className="plan-page__actions">
      {control !== null && (
        <div>
          <StartRunButton
            path={control.path}
            blockedBy={control.reason}
            look={checklist.primary?.kind === "start_run" ? "primary" : "secondary"}
          />
          <ConfigBlockNotice context="plan" />
        </div>
      )}
      {dashboard !== null && <DashboardLink url={dashboard} />}
    </div>
  );
}

/**
 * The server pushes no run state, so the page asks for it on arrival, as the run dialog does. A
 * failed request reads as failed, never as idle. An answer is applied even when a newer request
 * overtook it while no state is known (`fetchRunState`), so an answered request always ends the
 * wait, whatever happens to the newer one.
 */
function useRunRead(): RunRead {
  const store = useStore();
  const [read, setRead] = useState<RunRead>("pending");
  useEffect(() => {
    let live = true;
    void fetchRunState(store).then((result) => {
      if (!live) return;
      setRead(result.ok ? "read" : "failed");
    });
    return () => {
      live = false;
    };
  }, [store]);
  return read;
}

/** `/f/:feature/run`: the run's state, the one start action and each plan's state. */
export function RunPage() {
  const { checklist } = useFeature();
  const state = useAtomValue(runStateAtom);
  const strip = useAtomValue(runStripAtom);
  const read = useRunRead();
  if (!checklist) return <LoadState state="loading" title="Loading the feature" />;
  const sentence = runSentence(checklist, state, strip, read);
  return (
    <PlanPage
      feature={checklist.feature}
      label="run"
      title="Run"
      lede={
        <span role="status">
          <Untrusted text={sentence} />
        </span>
      }
    >
      {checklist.plan_rows.length === 0 ? (
        <NoPlans
          feature={checklist.feature}
          sentence="Write a plan first. A run builds a written plan, stage by stage."
        />
      ) : (
        <>
          <RunActions checklist={checklist} read={read} />
          <PlanRows checklist={checklist} />
          <UnwrittenPlans checklist={checklist} />
        </>
      )}
    </PlanPage>
  );
}
