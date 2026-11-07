import { useAtomValue, useStore } from "jotai/react";
import { useEffect, useState } from "react";

import type { ChecklistView } from "@/plan/api/schema";
import { ConfigBlockNotice } from "@/plan/chat/pressure/config-block-notice";
import { Untrusted } from "@/plan/chat/visible-controls";
import { runScope, runSentence, startControl, type RunRead } from "@/plan/feature/run-scope";
import { useFeature } from "@/plan/feature/use-feature";
import { DashboardLink, webUrl } from "@/plan/run/dashboard-button";
import { StartRunButton } from "@/plan/run/plan-run-actions";
import { fetchRunState, runStateAtom, runStripAtom } from "@/plan/run/use-run";

import "./run-panel.css";

/**
 * The panel's one Start a run button (`startControl`), with the Codex configuration notice under
 * it, and a dashboard link while a run of this feature (or none) is up. It is the screen's
 * primary only when the checklist's primary is the start_run offer.
 */
function RunActions({ checklist, read }: { checklist: ChecklistView; read: RunRead }) {
  const state = useAtomValue(runStateAtom);
  const control = startControl(checklist, state, read);
  const dashboard = runScope(checklist, state) === "foreign" ? null : webUrl(state?.dashboard_url);
  if (control === null && dashboard === null) return null;
  return (
    <div className="plan-run__actions">
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
 * The server pushes no run state, so the panel asks for it on arrival, as the run dialog does. A
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

function Panel({ checklist }: { checklist: ChecklistView }) {
  const state = useAtomValue(runStateAtom);
  const strip = useAtomValue(runStripAtom);
  const read = useRunRead();
  const sentence = runSentence(checklist, state, strip, read);
  return (
    <section className="plan-run" aria-label="Run">
      <p className="plan-run__line" role="status">
        <Untrusted text={sentence} />
      </p>
      <RunActions checklist={checklist} read={read} />
    </section>
  );
}

/**
 * The feature's run on the Plans overview: one line on the run's state, the one Start a run
 * button and the dashboard link. A plan's state is its spine row's and its page's, so the panel
 * lists no plans; a feature with no plan yet has no run to speak of and draws nothing.
 */
export function RunPanel() {
  const { checklist } = useFeature();
  if (!checklist || checklist.plan_rows.length === 0) return null;
  return <Panel checklist={checklist} />;
}
