import { ArrowLeftIcon, ArrowRightIcon } from "lucide-react";

import { Kbd } from "@/components/ui/kbd";
import { PLANNER_HELP_TOPICS, type PlannerHelpTopic } from "@/plan/help/topics";

/** Previous and next topic cards at the sides, with the open and close keys between them. */
export function TopicFooter({
  topic,
  onSelect,
}: {
  topic: PlannerHelpTopic;
  onSelect: (topic: PlannerHelpTopic) => void;
}) {
  const index = PLANNER_HELP_TOPICS.indexOf(topic);
  return (
    <footer className="planner-help-footer">
      <nav className="planner-help-pager" aria-label="Adjacent help topics">
        <PagerCard
          topic={PLANNER_HELP_TOPICS[index - 1]}
          direction="previous"
          onSelect={onSelect}
        />
        <PagerCard topic={PLANNER_HELP_TOPICS[index + 1]} direction="next" onSelect={onSelect} />
      </nav>
      <p className="planner-help-key-hint">
        <Kbd>?</Kbd> opens this guide <span aria-hidden="true">·</span> <Kbd>Esc</Kbd> closes
      </p>
    </footer>
  );
}

function PagerCard({
  topic,
  direction,
  onSelect,
}: {
  topic: PlannerHelpTopic | undefined;
  direction: "previous" | "next";
  onSelect: (topic: PlannerHelpTopic) => void;
}) {
  if (!topic) return null;
  const Icon = direction === "previous" ? ArrowLeftIcon : ArrowRightIcon;
  const label = direction === "previous" ? "Previous topic" : "Next topic";
  return (
    <button
      type="button"
      className={`planner-help-pager-card planner-help-pager-${direction}`}
      aria-label={`${label}: ${topic.label}`}
      onClick={() => onSelect(topic)}
    >
      <Icon aria-hidden="true" />
      <span>
        <small>{label}</small>
        <strong>{topic.label}</strong>
        <span>{topic.description}</span>
      </span>
    </button>
  );
}
