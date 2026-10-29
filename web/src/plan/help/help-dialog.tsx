import { useAtomValue } from "jotai/react";
import { ArrowLeftIcon, ArrowRightIcon, LightbulbIcon, SearchIcon } from "lucide-react";
import { useEffect, useId, useLayoutEffect, useRef, useState, type CSSProperties } from "react";

import { Logo } from "@/components/logo";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Kbd } from "@/components/ui/kbd";
import { isTyping } from "@/lib/is-typing";
import { useMediaQuery } from "@/lib/use-media-query";
import { PinnedScene } from "@/plan/help/pinned-scene";
import { PLANNER_HELP_TOPICS, type PlannerHelpTopic } from "@/plan/help/topics";
import { readingFaceAtom, readingSizeAtom } from "@/plan/settings/preferences";
import "./help-dialog.css";

export function PlannerHelpDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const face = useAtomValue(readingFaceAtom);
  const size = useAtomValue(readingSizeAtom);
  // Keyboard and header openers share this dialog, which has no Radix trigger.
  const returnFocus = useRef<HTMLElement | null>(null);
  const search = useHelpSearch(open);
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        data-loom-plan
        data-reading-face={face}
        data-reading-size={String(size)}
        className="planner-help"
        onEscapeKeyDown={search.onEscape}
        onOpenAutoFocus={() => {
          returnFocus.current =
            document.activeElement instanceof HTMLElement ? document.activeElement : null;
          search.setQuery("");
        }}
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          returnFocus.current?.focus();
        }}
      >
        <DialogDescription className="sr-only">
          A guide to planning, reading, and running your work in Loom.
        </DialogDescription>
        <HelpBrowser search={search} />
      </DialogContent>
    </Dialog>
  );
}

function useHelpSearch(open: boolean) {
  const [query, setQuery] = useState("");
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing || event.key !== "/") return;
      if (event.metaKey || event.ctrlKey || event.altKey || isTyping(event.target)) return;
      event.preventDefault();
      input.current?.focus();
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [open]);
  const onEscape = (event: KeyboardEvent) => {
    if (document.activeElement !== input.current || !query) return;
    event.preventDefault();
    setQuery("");
  };
  return { query, setQuery, input, onEscape };
}

function HelpBrowser({ search }: { search: ReturnType<typeof useHelpSearch> }) {
  const [selected, setSelected] = useState<PlannerHelpTopic | null>(null);
  const mobile = useMediaQuery("(max-width: 767px)");
  const query = search.query.trim().toLowerCase();
  const topics = PLANNER_HELP_TOPICS.filter((item) =>
    [item.label, item.description, item.intro].some((text) => text.toLowerCase().includes(query)),
  );
  const topic = topics.find((item) => item.id === selected?.id) ?? topics[0];
  const showArticle = !mobile || selected !== null || topics.length === 0;
  return (
    <div className="planner-help-browser">
      <TopicNavigation
        search={search}
        topics={topics}
        active={showArticle ? topic?.id : undefined}
        showTopics={!mobile || selected === null}
        onSelect={setSelected}
        onSearch={(value) => {
          search.setQuery(value);
          if (mobile) setSelected(null);
        }}
      />
      {showArticle && (
        <TopicDetail topic={topic} mobile={mobile} search={search} onSelect={setSelected} />
      )}
    </div>
  );
}

interface TopicDetailProps {
  topic: PlannerHelpTopic | undefined;
  mobile: boolean;
  search: ReturnType<typeof useHelpSearch>;
  onSelect: (topic: PlannerHelpTopic | null) => void;
}

function TopicDetail({ topic, mobile, search, onSelect }: TopicDetailProps) {
  const article = useRef<HTMLElement>(null);
  const titleId = useId();
  useLayoutEffect(() => {
    if (article.current) article.current.scrollTop = 0;
    if (mobile) article.current?.focus();
  }, [topic?.id, mobile]);
  return (
    <div className="planner-help-detail">
      {mobile && topic && (
        <button
          type="button"
          className="planner-help-back"
          onClick={() => {
            onSelect(null);
            search.input.current?.focus();
          }}
        >
          <ArrowLeftIcon aria-hidden="true" /> All topics
        </button>
      )}
      {topic ? (
        <article
          ref={article}
          tabIndex={0}
          aria-labelledby={titleId}
          className="planner-help-article"
        >
          <TopicContent key={topic.id} topic={topic} titleId={titleId} />
          <TopicFooter
            topic={topic}
            onSelect={(next) => {
              search.setQuery("");
              onSelect(next);
            }}
          />
        </article>
      ) : (
        <EmptySearch onClear={() => search.setQuery("")} />
      )}
    </div>
  );
}

interface TopicNavigationProps {
  search: ReturnType<typeof useHelpSearch>;
  topics: readonly PlannerHelpTopic[];
  active?: string;
  showTopics: boolean;
  onSelect: (topic: PlannerHelpTopic) => void;
  onSearch: (value: string) => void;
}

function TopicNavigation({
  search,
  topics,
  active,
  showTopics,
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
      {showTopics && (
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
      )}
    </aside>
  );
}

function TopicSearch({
  search,
  onSearch,
}: {
  search: ReturnType<typeof useHelpSearch>;
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
        placeholder="Search topics…"
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

function topicHue(topic: PlannerHelpTopic): CSSProperties {
  return { "--planner-help-hue": `var(--doc-${topic.hue})` } as CSSProperties;
}

function TopicContent({ topic, titleId }: { topic: PlannerHelpTopic; titleId: string }) {
  return (
    <div className="planner-help-content" style={topicHue(topic)}>
      <TopicHero topic={topic} titleId={titleId} />
      {topic.scenes.map((scene, index) => (
        <PinnedScene
          key={scene.title}
          number={index + 1}
          title={scene.title}
          notes={scene.notes}
          scale={scene.scale}
        >
          <scene.Scene />
        </PinnedScene>
      ))}
      {topic.Body && <topic.Body />}
      {topic.tips && topic.tips.length > 0 && <TopicTips tips={topic.tips} />}
    </div>
  );
}

function TopicHero({ topic, titleId }: { topic: PlannerHelpTopic; titleId: string }) {
  return (
    <header className="planner-help-hero">
      <span className="planner-help-badge">
        <topic.Icon aria-hidden="true" />
      </span>
      <div>
        <p className="planner-help-position">
          Topic {PLANNER_HELP_TOPICS.indexOf(topic) + 1} of {PLANNER_HELP_TOPICS.length}
        </p>
        <h2 id={titleId} className="font-display">
          {topic.label}
        </h2>
      </div>
      <p className="planner-help-intro">{topic.intro}</p>
    </header>
  );
}

function TopicTips({ tips }: { tips: readonly string[] }) {
  return (
    <aside className="planner-help-tips">
      <h3>
        <span className="planner-help-badge">
          <LightbulbIcon aria-hidden="true" />
        </span>
        Good to know
      </h3>
      <ul>
        {tips.map((tip) => (
          <li key={tip}>{tip}</li>
        ))}
      </ul>
    </aside>
  );
}

function TopicFooter({
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
        <Kbd>?</Kbd> help <span aria-hidden="true">·</span> <Kbd>Esc</Kbd> close
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
  if (!topic) return <span />;
  const Icon = direction === "previous" ? ArrowLeftIcon : ArrowRightIcon;
  const label = direction === "previous" ? "Previous topic" : "Next topic";
  return (
    <button
      type="button"
      className={`planner-help-pager-${direction}`}
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

function EmptySearch({ onClear }: { onClear: () => void }) {
  return (
    <div className="planner-help-empty">
      <p role="status">No topics match your search.</p>
      <button type="button" onClick={onClear}>
        Clear search
      </button>
    </div>
  );
}
