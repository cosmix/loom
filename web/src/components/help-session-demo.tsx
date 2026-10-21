import { useState } from "react";
import { GraphDemo } from "@/components/help-graph-demo";
import { HELP_SNAPSHOT, HELP_STAGE, HelpExample } from "@/components/help-example";
import { ModeKey } from "@/components/terminal/mode-key";
import { ContextMeter } from "@/components/context-meter";
import { QuotaMeters } from "@/components/quota-meters";
import { TokenFlowPanel } from "@/components/token-flow-panel";
import { StateBadge } from "@/components/state-badge";
import { NodeTokens } from "@/components/graph/node-tokens";
import { stageFlow } from "@/lib/tokens";

export function StageAnatomy() {
  return <GraphDemo single />;
}

export function TerminalDemo() {
  const [mode, setMode] = useState<"view" | "control">("view");
  return (
    <HelpExample>
      <figure className="help-figure">
        <div className="help-terminal-demo terminal-frame tone-executing" data-mode={mode}>
          <header className="terminal-head">
            <div>
              <strong>{HELP_STAGE.name}</strong>
              <div className="mt-2">
                <StateBadge status="executing" />
              </div>
            </div>
            <div className="terminal-controls">
              <ModeKey mode={mode} phase="live" switchable onMode={setMode} />
            </div>
          </header>
          <div className="help-terminal-screen terminal-well" data-mode={mode} data-phase="live">
            <div className="terminal-tape" aria-hidden="true" />
            <span>Agent session</span>
            <p>Which endpoint should I implement first?</p>
            <div>
              <span className="help-terminal-prompt">❯</span>{" "}
              <span className="help-terminal-cursor" />
            </div>
          </div>
          <footer className="terminal-foot">
            <span className="terminal-hint">
              {mode === "control" ? "controlling · keys reach the session" : "viewing · read-only"}
            </span>
          </footer>
        </div>
        <figcaption aria-live="polite">
          {mode === "control"
            ? "In a live terminal, Esc and Tab go to the agent. Use release to return to viewing."
            : "Try it: flip the switch. This example never connects to a session."}
        </figcaption>
      </figure>
    </HelpExample>
  );
}

export function ResourceDiagram() {
  const tokens = HELP_SNAPSHOT.status.tokens;
  const flow = stageFlow(tokens, HELP_STAGE.id);
  const quota = HELP_SNAPSHOT.status.quota;
  const now = Math.max(quota.claude?.observed_at ?? 0, quota.codex?.observed_at ?? 0);
  return (
    <HelpExample>
      <figure className="help-figure">
        <div className="help-meter-example">
          <h4>On a stage card</h4>
          <div className="help-stage-instruments tone-executing">
            <ContextMeter stage={HELP_STAGE} />
            {flow && <NodeTokens stage={HELP_STAGE} tokens={flow} />}
          </div>
        </div>
        <div className="help-token-panel">
          <TokenFlowPanel />
        </div>
        <div className="help-quota-example">
          <h4>In the footer</h4>
          <QuotaMeters snapshot={quota} quotaLeft={tokens?.quota_left ?? []} nowSecs={now} />
        </div>
      </figure>
    </HelpExample>
  );
}
