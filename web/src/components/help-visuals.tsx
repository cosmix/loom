import {
  CheckIcon,
  GitMergeIcon,
  Rows3Icon,
  WorkflowIcon,
  SlidersHorizontalIcon,
  CircleHelpIcon,
  CopyIcon,
} from "lucide-react";
import { GraphDemo } from "@/components/help-graph-demo";
import type { HelpTopic } from "@/components/help-topics";
import { StageAnatomy, TerminalDemo, ResourceDiagram } from "@/components/help-session-demo";
import { HelpExample, HELP_SNAPSHOT } from "@/components/help-example";
import { Button } from "@/components/ui/button";
import { ThemeToggle } from "@/aurora-ui/theme/ThemeToggle";
import { HazardPanel } from "@/aurora-ui/feedback/HazardPanel";
import { CopyCommand } from "@/components/copy-command";
import { DaemonLine } from "@/components/header-lines";
import "./help-visuals.css";

export function HelpVisual({ topic }: { topic: HelpTopic["id"] }) {
  switch (topic) {
    case "start":
      return <RunDiagram />;
    case "graph":
      return <GraphDemo />;
    case "stages":
      return <StageAnatomy />;
    case "terminals":
      return <TerminalDemo />;
    case "tokens":
      return <ResourceDiagram />;
    case "settings":
      return <SettingsDiagram />;
    case "shortcuts":
      return <HeaderControls />;
    case "attention":
      return <AttentionDiagram />;
    case "activity":
      return <ConnectionDiagram />;
    default:
      return null;
  }
}

function RunDiagram() {
  return (
    <figure className="help-figure">
      <div className="help-figure-label">
        <span>Stage lifecycle</span>
        <span>Successful path</span>
      </div>
      <ol className="help-run-path">
        <li>
          <span>01</span>
          <strong>Waiting</strong>
          <small>Dependencies</small>
        </li>
        <li>
          <span>02</span>
          <strong>Queued</strong>
          <small>Execution slot</small>
        </li>
        <li>
          <span>03</span>
          <strong>Executing</strong>
          <small>Agent session</small>
        </li>
        <li>
          <span>
            <CheckIcon />
          </span>
          <strong>Completed</strong>
          <small>Work verified</small>
        </li>
        <li>
          <span>
            <GitMergeIcon />
          </span>
          <strong>Merged</strong>
          <small>Changes integrated</small>
        </li>
      </ol>
      <figcaption>
        <GitMergeIcon />
        <span>
          <strong>Completed + merged</strong> releases downstream stages.
        </span>
      </figcaption>
      <HeaderControls compact />
    </figure>
  );
}

function HeaderControls({ compact = false }: { compact?: boolean }) {
  return (
    <div className={`help-header-key ${compact ? "is-compact" : ""}`}>
      <p className="help-figure-label">Find it in the header</p>
      <dl>
        <div>
          <dt className="view-switch" aria-label="Example view switch">
            <span className="view-tab is-active">
              <WorkflowIcon className="size-3.5" />
              graph
            </span>
            <span className="view-tab">
              <Rows3Icon className="size-3.5" />
              ledger
            </span>
          </dt>
          <dd>Switch views</dd>
        </div>
        <div>
          <dt>
            <div inert>
              <ThemeToggle />
            </div>
          </dt>
          <dd>Change theme</dd>
        </div>
        {[
          { Icon: SlidersHorizontalIcon, label: "settings", description: "Configure Loom" },
          { Icon: CircleHelpIcon, label: "help", description: "Open this guide" },
        ].map(({ Icon, label, description }) => (
          <div key={label}>
            <dt>
              <Button asChild variant="ghost" size="sm">
                <span>
                  <Icon />
                  {label}
                </span>
              </Button>
            </dt>
            <dd>{description}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

function SettingsDiagram() {
  return (
    <figure className="help-figure">
      <div className="help-figure-label">
        <span>Configuration precedence</span>
        <span>Highest priority first</span>
      </div>
      <ol className="help-scope-path">
        <li>
          <strong>Project</strong>
          <span>This workspace’s override</span>
        </li>
        <li>
          <strong>User</strong>
          <span>Your fallback across projects</span>
        </li>
        <li>
          <strong>Default</strong>
          <span>Loom’s built-in value</span>
        </li>
      </ol>
      <figcaption>
        <SlidersHorizontalIcon />
        <span>Removing an override falls back to the next available value.</span>
      </figcaption>
      <div className="help-local-settings">
        <SlidersHorizontalIcon />
        <p>
          <strong>Theme & notifications</strong>Saved in this browser, outside the configuration
          scopes above.
        </p>
      </div>
    </figure>
  );
}

function AttentionDiagram() {
  return (
    <figure className="help-figure">
      <div className="help-figure-label">
        <span>Read the guidance before acting</span>
      </div>
      <HazardPanel
        tone="error"
        title="ACCEPTANCE FAILED"
        className="bg-card"
        bodyClassName="flex flex-col gap-2.5 text-sm"
      >
        <p>
          <strong>Example stage</strong> <code className="text-xs text-muted-foreground">api</code>
        </p>
        <p>Read the failure evidence and fix the underlying problem before retrying.</p>
        <CopyCommand command="loom stage retry <id>" />
      </HazardPanel>
      <div className="help-automatic-example">
        <GitMergeIcon />
        <p>
          <strong>Handled by loom</strong>Recovery or review is already running automatically.
        </p>
      </div>
      <figcaption>
        <CopyIcon />
        <span>The copy button puts the command on your clipboard. Run it in your own shell.</span>
      </figcaption>
    </figure>
  );
}

function ConnectionDiagram() {
  return (
    <figure className="help-figure">
      <div className="help-figure-label">
        <span>Read the connection separately from the daemon</span>
      </div>
      <dl className="help-connection-key">
        <div>
          <dt>
            <HelpExample>
              <DaemonLine snapshot={HELP_SNAPSHOT} />
            </HelpExample>
          </dt>
          <dd>New snapshots show current activity.</dd>
        </div>
        <div>
          <dt>
            <HelpExample>
              <DaemonLine snapshot={{ ...HELP_SNAPSHOT, daemon: "not-running" }} />
            </HelpExample>
          </dt>
          <dd>The connection works; the daemon is stopped.</dd>
        </div>
        <div>
          <dt>
            <HelpExample phase="offline">
              <DaemonLine snapshot={HELP_SNAPSHOT} />
            </HelpExample>
          </dt>
          <dd>The last snapshot stays visible while reconnecting.</dd>
        </div>
      </dl>
    </figure>
  );
}
