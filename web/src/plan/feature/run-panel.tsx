import { useAtomValue, useStore } from "jotai/react";
import { ArrowRight, Circle, CircleCheck } from "lucide-react";
import { useEffect, useId, useState, type JSX } from "react";
import { Link } from "react-router";

import type { ChecklistView } from "@/plan/api/schema";
import { ConfigBlockNotice } from "@/plan/chat/pressure/config-block-notice";
import { Untrusted } from "@/plan/chat/visible-controls";
import {
  BlockedReason,
  LedgerRow,
  LedgerStatus,
  type LedgerTone,
} from "@/plan/concept/header-ledger";
import {
  allPlansDone,
  runScope,
  runSentence,
  startControl,
  type RunRead,
} from "@/plan/feature/run-scope";
import { featureHref, useFeature } from "@/plan/feature/use-feature";
import { DashboardLink, webUrl } from "@/plan/run/dashboard-button";
import { StartRunButton, useStartRunReason } from "@/plan/run/plan-run-actions";
import { fetchRunState, runStateAtom, runStripAtom } from "@/plan/run/use-run";
import { openCount } from "@/plan/trace/findings-model";
import { useTrace } from "@/plan/trace/state";
import { INSTRUMENT_GLYPHS } from "@/plan/ui/glyphs";
import { ArrowSpinner } from "@/plan/ui/spinner";

import "./run-panel.css";

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

/** The run's tone and glyph: turning while a run of this feature works, a check once all is done. */
function runLook(
  checklist: ChecklistView,
  workspace: string | undefined,
  own: boolean,
): { tone: LedgerTone; glyph: JSX.Element } {
  if (own && workspace === "running") return { tone: "executing", glyph: <ArrowSpinner /> };
  if (allPlansDone(checklist)) return { tone: "completed", glyph: <CircleCheck /> };
  return { tone: "pending", glyph: <Circle /> };
}

/**
 * The Run row of the metaplan's ledger: the one line on the run's state, the one Start a run
 * button and the dashboard link at the row's end, and what blocks the button as a lock-led line
 * under the status. The button is the screen's primary only when the checklist's primary is the
 * start_run offer; the Codex configuration notice follows the status.
 */
function RunLedgerRow({ checklist }: { checklist: ChecklistView }): JSX.Element {
  const state = useAtomValue(runStateAtom);
  const strip = useAtomValue(runStripAtom);
  const read = useRunRead();
  const reasonId = useId();
  const control = startControl(checklist, state, read);
  const reason = useStartRunReason(control?.reason ?? null);
  const scope = runScope(checklist, state);
  const dashboard = scope === "foreign" ? null : webUrl(state?.dashboard_url);
  const { tone, glyph } = runLook(checklist, state?.workspace, scope === "own");
  const action =
    control === null && dashboard === null ? null : (
      <>
        {control !== null && (
          <StartRunButton
            path={control.path}
            blockedBy={control.reason}
            reasonId={reasonId}
            look={checklist.primary?.kind === "start_run" ? "primary" : "secondary"}
          />
        )}
        {dashboard !== null && (
          <span className="header-ledger__dashboard">
            <DashboardLink url={dashboard} />
          </span>
        )}
      </>
    );
  return (
    <LedgerRow label="Run" action={action}>
      <LedgerStatus tone={tone} glyph={glyph} role="status">
        <Untrusted text={runSentence(checklist, state, strip, read)} />
      </LedgerStatus>
      {control !== null && reason !== null && (
        <BlockedReason id={reasonId}>
          <Untrusted text={reason} />
        </BlockedReason>
      )}
      {control !== null && (
        <div className="header-ledger__notes">
          <ConfigBlockNotice context="plan" />
        </div>
      )}
    </LedgerRow>
  );
}

/**
 * The Findings row: how many findings wait, and the way to the Findings page; drawn once the
 * feature's ledger holds any finding. With none open the button is quiet.
 */
function FindingsLedgerRow({ feature }: { feature: string }): JSX.Element | null {
  const { trace } = useTrace(feature);
  if (trace === null || trace.findings.length === 0) return null;
  const open = openCount(trace);
  const Glyph = INSTRUMENT_GLYPHS.findings;
  return (
    <LedgerRow
      label="Findings"
      action={
        <Link
          to={featureHref(feature, "findings")}
          data-look={open > 0 ? "secondary" : "quiet"}
          className="plan-button inline-flex shrink-0 items-center justify-center"
        >
          Open findings
          <ArrowRight aria-hidden="true" />
        </Link>
      }
    >
      <LedgerStatus tone={open > 0 ? "warning" : undefined} glyph={<Glyph />}>
        {open} open
      </LedgerStatus>
    </LedgerRow>
  );
}

/** The feature's run on the metaplan page, the Plans home; a feature with no plan has none. */
export function RunRow(): JSX.Element | null {
  const { checklist } = useFeature();
  if (!checklist || checklist.plan_rows.length === 0) return null;
  return <RunLedgerRow checklist={checklist} />;
}

/** The feature's findings on the metaplan page, after the settling row. */
export function FindingsRow(): JSX.Element | null {
  const { checklist } = useFeature();
  if (!checklist || checklist.plan_rows.length === 0) return null;
  return <FindingsLedgerRow feature={checklist.feature} />;
}
