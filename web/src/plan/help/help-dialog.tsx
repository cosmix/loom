import { useAtomValue } from "jotai/react";
import { LightbulbIcon } from "lucide-react";
import { useId, useLayoutEffect, useRef, useState } from "react";

import { Dialog, DialogContent, DialogDescription } from "@/components/ui/dialog";
import { useMediaQuery } from "@/lib/use-media-query";
import { TopicFooter } from "@/plan/help/help-dialog-footer";
import { TopicNavigation, topicHue } from "@/plan/help/help-dialog-nav";
import { useHelpSearch, type HelpSearch } from "@/plan/help/help-search";
import { PinnedScene } from "@/plan/help/pinned-scene";
import { PLANNER_HELP_TOPICS, type PlannerHelpTopic } from "@/plan/help/topics";
import { readingFaceAtom, readingSizeAtom } from "@/plan/settings/preferences";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import "./help-dialog.css";
import "./help-dialog-topic.css";

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

function HelpBrowser({ search }: { search: HelpSearch }) {
  const [selected, setSelected] = useState<PlannerHelpTopic | null>(null);
  // Counts the topics the reader chose, so a typed search never scrolls the dialog away from it.
  const [picks, setPicks] = useState(0);
  const mobile = useMediaQuery("(max-width: 767px)");
  const query = search.query.trim().toLowerCase();
  const topics = PLANNER_HELP_TOPICS.filter((item) =>
    [item.label, item.description, item.intro].some((text) => text.toLowerCase().includes(query)),
  );
  const topic = topics.find((item) => item.id === selected?.id) ?? topics[0];
  const choose = (next: PlannerHelpTopic) => {
    setSelected(next);
    setPicks((count) => count + 1);
  };
  return (
    <div className="planner-help-browser">
      <TopicNavigation
        search={search}
        topics={topics}
        active={topic?.id}
        onSelect={choose}
        onSearch={search.setQuery}
      />
      <TopicDetail topic={topic} mobile={mobile} picks={picks} search={search} onSelect={choose} />
    </div>
  );
}

interface TopicDetailProps {
  topic: PlannerHelpTopic | undefined;
  mobile: boolean;
  picks: number;
  search: HelpSearch;
  onSelect: (topic: PlannerHelpTopic) => void;
}

function TopicDetail({ topic, mobile, picks, search, onSelect }: TopicDetailProps) {
  const article = useRef<HTMLElement>(null);
  const titleId = useId();
  useLayoutEffect(() => {
    if (article.current) article.current.scrollTop = 0;
  }, [topic?.id]);
  // On a phone the dialog scrolls as a whole, nav then topic, so a chosen topic is scrolled to.
  useLayoutEffect(() => {
    const node = article.current;
    if (picks === 0 || !mobile || !node) return;
    node.focus({ preventScroll: true });
    if (typeof node.scrollIntoView === "function") node.scrollIntoView({ block: "start" });
  }, [picks, mobile]);
  return (
    <div className="planner-help-detail">
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
          panel={scene.panel}
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
        <p className="plan-eyebrow planner-help-position">
          Topic {PLANNER_HELP_TOPICS.indexOf(topic) + 1} of {PLANNER_HELP_TOPICS.length}
        </p>
        <h2 id={titleId} className="plan-page-title">
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
      <span className="planner-help-tips-tile" aria-hidden="true">
        <LightbulbIcon />
      </span>
      <div>
        <h3>Good to know</h3>
        <ul>
          {tips.map((tip) => (
            <li key={tip}>{tip}</li>
          ))}
        </ul>
      </div>
    </aside>
  );
}

function EmptySearch({ onClear }: { onClear: () => void }) {
  return (
    <div className="planner-help-empty">
      <p role="status">No topics match your search.</p>
      <DisabledReason reason={null} onClick={onClear}>
        Clear search
      </DisabledReason>
    </div>
  );
}
