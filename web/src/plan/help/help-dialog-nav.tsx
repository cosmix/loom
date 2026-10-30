import { SearchIcon } from "lucide-react";
import type { CSSProperties } from "react";

import { Logo } from "@/components/logo";
import { DialogTitle } from "@/components/ui/dialog";
import { Kbd } from "@/components/ui/kbd";
import type { HelpSearch } from "@/plan/help/help-search";
import type { PlannerHelpTopic } from "@/plan/help/topics";

/** `--planner-help-hue` tints a topic's tile and its hero badge. */
export function topicHue(topic: PlannerHelpTopic): CSSProperties {
  return { "--planner-help-hue": `var(--doc-${topic.hue})` } as CSSProperties;
}

interface TopicNavigationProps {
  search: HelpSearch;
  topics: readonly PlannerHelpTopic[];
  active?: string;
  onSelect: (topic: PlannerHelpTopic) => void;
  onSearch: (value: string) => void;
}

export function TopicNavigation({
  search,
  topics,
  active,
  onSelect,
  onSearch,
}: TopicNavigationProps) {
  return (
    <aside className="planner-help-sidebar">
      <header className="planner-help-brand">
        <Logo className="planner-help-logo" />
        <DialogTitle className="font-display">Help</DialogTitle>
      </header>
      <TopicSearch search={search} onSearch={onSearch} />
      <nav className="planner-help-nav" aria-label="Help topics">
        {topics.map((topic) => (
          <TopicButton
            key={topic.id}
            topic={topic}
            active={topic.id === active}
            onSelect={onSelect}
          />
        ))}
      </nav>
    </aside>
  );
}

function TopicSearch({
  search,
  onSearch,
}: {
  search: HelpSearch;
  onSearch: (value: string) => void;
}) {
  return (
    <label className="planner-help-search">
      <SearchIcon aria-hidden="true" />
      <span className="sr-only">Search help</span>
      <input
        ref={search.input}
        type="search"
        aria-label="Search help"
        placeholder="Search help"
        value={search.query}
        onChange={(event) => onSearch(event.target.value)}
      />
      <Kbd>/</Kbd>
    </label>
  );
}

function TopicButton({
  topic,
  active,
  onSelect,
}: {
  topic: PlannerHelpTopic;
  active: boolean;
  onSelect: (topic: PlannerHelpTopic) => void;
}) {
  return (
    <button
      type="button"
      className="planner-help-topic"
      aria-current={active ? "page" : undefined}
      style={topicHue(topic)}
      onClick={() => onSelect(topic)}
    >
      <span className="planner-help-badge">
        <topic.Icon aria-hidden="true" />
      </span>
      <span className="planner-help-topic-copy">
        <strong>{topic.label}</strong>
        <span>{topic.description}</span>
      </span>
    </button>
  );
}
