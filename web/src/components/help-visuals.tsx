import {
  CheckIcon,
  CircleHelpIcon,
  GitMergeIcon,
  HourglassIcon,
  ListOrderedIcon,
  MonitorIcon,
  PlayIcon,
  Rows3Icon,
  SlidersHorizontalIcon,
  WorkflowIcon,
  type LucideIcon,
} from "lucide-react";
import { GraphDemo, LedgerDemo } from "@/components/help-graph-demo";
import type { HelpVisualId } from "@/components/help-topics";
import { StageAnatomy, TerminalDemo, ResourceDiagram } from "@/components/help-session-demo";
import { HelpExample, HELP_SNAPSHOT } from "@/components/help-example";
import { toneClass } from "@/components/state-badge";
import { Button } from "@/components/ui/button";
import { ThemeToggle } from "@/aurora-ui/theme/ThemeToggle";
import { HazardPanel } from "@/aurora-ui/feedback/HazardPanel";
import { CopyCommand } from "@/components/copy-command";
import { DaemonLine } from "@/components/header-lines";
import type { Tone } from "@/lib/format";
import "./help-visuals.css";

export function HelpVisual({ visual }: { visual: HelpVisualId }) {
  switch (visual) {
    case "lifecycle":
      return <Lifecycle />;
    case "header":
      return <HeaderControls />;
    case "graph":
      return <GraphDemo />;
    case "ledger":
      return <LedgerDemo />;
    case "card":
      return <StageAnatomy />;
    case "attention":
      return <AttentionDiagram />;
    case "connection":
      return <ConnectionDiagram />;
    case "terminal":
      return <TerminalDemo />;
    case "resources":
      return <ResourceDiagram />;
    case "scopes":
      return <SettingsDiagram />;
  }
}

const LIFECYCLE: { pin: string; label: string; tone: Tone; Icon: LucideIcon }[] = [
  { pin: "waiting", label: "Waiting", tone: "pending", Icon: HourglassIcon },
  { pin: "queued", label: "Queued", tone: "queued", Icon: ListOrderedIcon },
  { pin: "executing", label: "Executing", tone: "executing", Icon: PlayIcon },
  { pin: "completed", label: "Completed", tone: "completed", Icon: CheckIcon },
  { pin: "merged", label: "Merged", tone: "merged", Icon: GitMergeIcon },
];

function Lifecycle() {
  return (
    <ol className="help-run-path">
      {LIFECYCLE.map(({ pin, label, tone, Icon }) => (
        <li key={pin} className={toneClass(tone)}>
          <span data-pin={pin}>
            <Icon aria-hidden="true" />
          </span>
          <strong>{label}</strong>
        </li>
      ))}
    </ol>
  );
}

function HeaderControls() {
  return (
    <div className="help-header-strip">
      <span className="view-switch" data-pin="views" aria-label="Example view switch">
        <span className="view-tab is-active">
          <WorkflowIcon className="size-3.5" />
          graph
        </span>
        <span className="view-tab">
          <Rows3Icon className="size-3.5" />
          ledger
        </span>
      </span>
      <span data-pin="theme" inert>
        <ThemeToggle />
      </span>
      {[
        { Icon: SlidersHorizontalIcon, label: "settings" },
        { Icon: CircleHelpIcon, label: "help" },
      ].map(({ Icon, label }) => (
        <Button key={label} asChild variant="ghost" size="sm" data-pin={label}>
          <span>
            <Icon />
            {label}
          </span>
        </Button>
      ))}
    </div>
  );
}

function SettingsDiagram() {
  return (
    <div className="help-scopes">
      <ol className="help-scope-path">
        <li data-pin="project">
          <strong>Project</strong>
          <span>this workspace</span>
        </li>
        <li data-pin="user">
          <strong>User</strong>
          <span>all your projects</span>
        </li>
        <li data-pin="default">
          <strong>Default</strong>
          <span>built in</span>
        </li>
      </ol>
      <div className="help-local-settings" data-pin="browser">
        <MonitorIcon aria-hidden="true" />
        <strong>This browser</strong>
        <span>theme · notifications</span>
      </div>
    </div>
  );
}

function AttentionDiagram() {
  return (
    <div className="help-attention">
      <div data-pin="problem">
        <HazardPanel
          tone="error"
          title="ACCEPTANCE FAILED"
          className="bg-card"
          bodyClassName="flex flex-col gap-2.5 text-sm"
        >
          <p>
            <strong>Example stage</strong>{" "}
            <code className="text-xs text-muted-foreground">api</code>
          </p>
          <p className="self-start" data-pin="guidance">
            Read the failure evidence and fix the underlying problem before retrying.
          </p>
          <span className="self-start" data-pin="copy">
            <CopyCommand command="loom stage retry <id>" />
          </span>
        </HazardPanel>
      </div>
      <div className="help-automatic-example" data-pin="auto">
        <GitMergeIcon aria-hidden="true" />
        <strong>Handled by loom</strong>
        <span>merge resolution running</span>
      </div>
    </div>
  );
}

function ConnectionDiagram() {
  return (
    <div className="help-connection-key">
      <span data-pin="live">
        <HelpExample>
          <DaemonLine snapshot={HELP_SNAPSHOT} />
        </HelpExample>
      </span>
      <span data-pin="stopped">
        <HelpExample>
          <DaemonLine snapshot={{ ...HELP_SNAPSHOT, daemon: "not-running" }} />
        </HelpExample>
      </span>
      <span data-pin="offline">
        <HelpExample phase="offline">
          <DaemonLine snapshot={HELP_SNAPSHOT} />
        </HelpExample>
      </span>
    </div>
  );
}
