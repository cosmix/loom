import { Link } from "react-router";

import type { StageSummary } from "@/api/schema";
import { ContextMeter } from "@/components/context-meter";
import { Field, Section, secs, tokens, yesNo } from "@/components/stage-detail";
import { stageHref } from "@/components/stage-href";
import { Badge } from "@/components/ui/badge";
import { failureLabel, modelsOf } from "@/lib/format";

export function IdentitySection({ stage }: { stage: StageSummary }) {
  const models = modelsOf(stage);
  return (
    <Section title="identity">
      <Field label="name">{stage.name}</Field>
      <Field label="id" mono>
        {stage.id}
      </Field>
      <Field label="type" mono>
        {stage.stage_type}
      </Field>
      <Field label="model" mono>
        {models.model}
      </Field>
      <Field label="execution models" mono>
        {models.execution.length > 0 ? models.execution.join(", ") : null}
      </Field>
    </Section>
  );
}

export function GraphSection({ stage, level }: { stage: StageSummary; level: number | null }) {
  return (
    <Section title="graph">
      <Field label="depends on">
        {stage.dependencies.length > 0 ? (
          <span className="flex flex-wrap gap-1">
            {stage.dependencies.map((id) => (
              <Badge
                key={id}
                variant="secondary"
                asChild
                className="font-mono text-[11px] font-normal"
              >
                <Link to={stageHref(id)}>{id}</Link>
              </Badge>
            ))}
          </span>
        ) : null}
      </Field>
      <Field label="level" mono>
        {level === null ? null : String(level)}
      </Field>
    </Section>
  );
}

export function TimingSection({ stage }: { stage: StageSummary }) {
  return (
    <Section title="timing">
      <Field label="elapsed" mono>
        {secs(stage.elapsed_secs)}
      </Field>
      <Field label="execution" mono>
        {secs(stage.execution_secs)}
      </Field>
    </Section>
  );
}

export function ContextSection({ stage }: { stage: StageSummary }) {
  return (
    <Section title="context">
      <Field label="tokens" mono>
        {tokens(stage.context_tokens)}
      </Field>
      <Field label="ceiling" mono>
        {tokens(stage.context_ceiling_tokens)}
      </Field>
      <Field label="usage">
        <ContextMeter stage={stage} detail />
      </Field>
    </Section>
  );
}

export function SessionSection({ stage }: { stage: StageSummary }) {
  return (
    <Section title="session">
      <Field label="pid" mono>
        {stage.pid === null ? null : String(stage.pid)}
      </Field>
      <Field label="alive">{yesNo(stage.session_alive)}</Field>
      <Field label="backend" mono>
        {stage.session_backend}
      </Field>
      <Field label="session type" mono>
        {stage.session_type}
      </Field>
      <Field label="activity" mono>
        {stage.activity_status}
      </Field>
      <Field label="last tool" mono>
        {stage.last_tool}
      </Field>
      <Field label="last activity" mono>
        {stage.last_activity}
      </Field>
      <Field label="staleness" mono>
        {secs(stage.staleness_secs)}
      </Field>
    </Section>
  );
}

export function RetriesSection({ stage }: { stage: StageSummary }) {
  return (
    <Section title="retries">
      <Field label="retry count" mono>
        {String(stage.retry_count)}
      </Field>
      <Field label="max retries" mono>
        {stage.max_retries === null ? null : String(stage.max_retries)}
      </Field>
    </Section>
  );
}

export function AdjudicationSection({ stage }: { stage: StageSummary }) {
  return (
    <Section title="adjudication">
      <Field label="disputes" mono>
        {String(stage.dispute_count)}
      </Field>
      <Field label="judge heartbeat" mono>
        {stage.judge_heartbeat_secs === null ? null : `${secs(stage.judge_heartbeat_secs)} ago`}
      </Field>
    </Section>
  );
}

export function MergeSection({ stage }: { stage: StageSummary }) {
  return (
    <Section title="merge">
      <Field label="merged">{yesNo(stage.merged)}</Field>
      <Field label="base branch" mono>
        {stage.base_branch}
      </Field>
      <Field label="base merged from" mono>
        {stage.base_merged_from.length > 0 ? stage.base_merged_from.join(", ") : null}
      </Field>
      <Field label="cleanup warning">{stage.cleanup_warning ?? null}</Field>
    </Section>
  );
}

export function NotesSection({ stage }: { stage: StageSummary }) {
  return (
    <Section title="notes">
      <Field label="review reason">{stage.review_reason}</Field>
      <Field label="incoherence" mono>
        {stage.incoherence}
      </Field>
      <Field label="held">{yesNo(stage.held)}</Field>
    </Section>
  );
}

/// Evidence is untrusted text: rendered as text nodes only.
export function FailureSection({ stage }: { stage: StageSummary }) {
  const failure = stage.failure_info;
  return (
    <Section title="failure">
      <Field label="type" mono>
        {failure ? `${failure.failure_type} (${failureLabel(failure.failure_type)})` : null}
      </Field>
      <Field label="detected at" mono>
        {failure?.detected_at ?? null}
      </Field>
      <Field label="evidence">
        {failure && failure.evidence.length > 0 ? (
          <pre className="max-h-64 overflow-auto rounded-md border border-hairline bg-background p-2.5 text-[11px] leading-relaxed whitespace-pre-wrap">
            {failure.evidence.join("\n")}
          </pre>
        ) : null}
      </Field>
    </Section>
  );
}
