import {
  ArrowLeftIcon,
  ArrowRightIcon,
  BookOpenIcon,
  ChevronRightIcon,
  LightbulbIcon,
} from "lucide-react";
import { useLayoutEffect, useRef, useState, type CSSProperties, type RefObject } from "react";

import { HelpKeys } from "@/components/help-keys";
import { SceneFigure } from "@/components/help-scene";
import { LegendTopic } from "@/components/help-legend";
import { HelpVisual } from "@/components/help-visuals";
import { HELP_TOPICS, type HelpShortcutGroup, type HelpTopic } from "@/components/help-topics";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Kbd } from "@/components/ui/kbd";
import { useMediaQuery } from "@/lib/use-media-query";
import "./help.css";

export function HelpDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const returnFocus = useRef<HTMLElement | null>(null);
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="help-dialog"
        onOpenAutoFocus={() => {
          returnFocus.current =
            document.activeElement instanceof HTMLElement ? document.activeElement : null;
        }}
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          returnFocus.current?.focus();
        }}
      >
        <header className="help-header">
          <BookOpenIcon className="size-5" aria-hidden="true" />
          <div>
            <DialogTitle>Loom help</DialogTitle>
            <DialogDescription>A guide to your running plan</DialogDescription>
          </div>
          <span className="help-key-hint">
            <Kbd>?</Kbd> help <span aria-hidden="true">·</span> <Kbd>Esc</Kbd> close
          </span>
        </header>
        <HelpBrowser />
      </DialogContent>
    </Dialog>
  );
}

function HelpBrowser() {
  const [selected, setSelected] = useState<HelpTopic | null>(null);
  const mobile = useMediaQuery("(max-width: 767px)");
  const article = useRef<HTMLElement>(null);
  const nav = useRef<HTMLElement>(null);
  const topic = selected ?? HELP_TOPICS[0];
  const showArticle = !mobile || selected !== null;
  const selectTopic = (next: HelpTopic | null) => {
    setSelected(next);
    if (!next) requestAnimationFrame(() => nav.current?.querySelector("button")?.focus());
  };
  useLayoutEffect(() => {
    if (article.current) article.current.scrollTop = 0;
    if (mobile && selected) article.current?.focus();
  }, [topic.id, mobile, selected]);
  return (
    <div className="help-browser">
      {(!mobile || !selected) && (
        <TopicNav navRef={nav} active={showArticle ? topic.id : null} onSelect={selectTopic} />
      )}
      {showArticle && (
        <div className="help-detail">
          {mobile && (
            <Button variant="ghost" className="help-back" onClick={() => selectTopic(null)}>
              <ArrowLeftIcon /> All topics
            </Button>
          )}
          <article
            ref={article}
            tabIndex={0}
            aria-labelledby="help-topic-title"
            className="help-article"
          >
            <TopicContent key={topic.id} topic={topic} />
            <TopicPager topic={topic} onSelect={selectTopic} />
          </article>
        </div>
      )}
    </div>
  );
}

function TopicNav({
  navRef,
  active,
  onSelect,
}: {
  navRef: RefObject<HTMLElement | null>;
  active: string | null;
  onSelect: (topic: HelpTopic) => void;
}) {
  return (
    <nav ref={navRef} className="help-nav" aria-label="Help topics">
      <p className="eyebrow help-nav-caption">Explore the dashboard</p>
      {HELP_TOPICS.map((item, i) => (
        <button
          key={item.id}
          type="button"
          aria-current={item.id === active ? "page" : undefined}
          onClick={() => onSelect(item)}
          className="help-topic-link"
        >
          <item.icon className="size-4 shrink-0" aria-hidden="true" />
          <span>{item.label}</span>
          <span className="help-topic-number" aria-hidden="true">
            {ordinal(i)}
          </span>
          <ChevronRightIcon className="help-chevron size-3.5" aria-hidden="true" />
        </button>
      ))}
    </nav>
  );
}

function ordinal(index: number) {
  return String(index + 1).padStart(2, "0");
}

function TopicContent({ topic }: { topic: HelpTopic }) {
  const index = HELP_TOPICS.indexOf(topic);
  return (
    <>
      <div className="help-topic-heading help-reveal">
        <span className="help-topic-numeral" aria-hidden="true">
          {ordinal(index)}
        </span>
        <p className="eyebrow">
          Web UI guide · {ordinal(index)} of {HELP_TOPICS.length}
        </p>
        <h2 id="help-topic-title">{topic.label}</h2>
        <p>{topic.intro}</p>
      </div>
      {topic.id === "legend" && <LegendTopic />}
      {topic.scenes.map((scene, i) => (
        <SceneFigure key={scene.title} scene={scene} index={i}>
          <HelpVisual visual={scene.visual} />
        </SceneFigure>
      ))}
      {topic.shortcuts && <ShortcutGroups groups={topic.shortcuts} />}
      {topic.tips && <Tips tips={topic.tips} />}
    </>
  );
}

function ShortcutGroups({ groups }: { groups: HelpShortcutGroup[] }) {
  return (
    <div className="help-shortcut-groups">
      {groups.map((group, i) => (
        <section
          key={group.title}
          className="help-shortcut-group help-reveal"
          style={{ "--i": i + 1 } as CSSProperties}
        >
          <h3>{group.title}</h3>
          <dl className="help-shortcuts">
            {group.rows.map(([keys, action]) => (
              <div key={`${keys}${action}`}>
                <dt>
                  <HelpKeys spec={keys} />
                </dt>
                <dd>{action}</dd>
              </div>
            ))}
          </dl>
        </section>
      ))}
    </div>
  );
}

function Tips({ tips }: { tips: string[] }) {
  return (
    <aside className="help-tips help-reveal" style={{ "--i": 4 } as CSSProperties}>
      <h3>
        <LightbulbIcon aria-hidden="true" /> Good to know
      </h3>
      <ul>
        {tips.map((tip) => (
          <li key={tip}>{tip}</li>
        ))}
      </ul>
    </aside>
  );
}

function TopicPager({
  topic,
  onSelect,
}: {
  topic: HelpTopic;
  onSelect: (topic: HelpTopic) => void;
}) {
  const index = HELP_TOPICS.indexOf(topic);
  const previous = HELP_TOPICS[index - 1];
  const next = HELP_TOPICS[index + 1];
  return (
    <footer className="help-pager">
      {previous && (
        <button type="button" onClick={() => onSelect(previous)}>
          <ArrowLeftIcon aria-hidden="true" />
          <span>
            <small>Previous topic</small>
            {previous.label}
          </span>
        </button>
      )}
      {next && (
        <button type="button" className="help-next" onClick={() => onSelect(next)}>
          <span>
            <small>Next topic</small>
            {next.label}
          </span>
          <ArrowRightIcon aria-hidden="true" />
        </button>
      )}
    </footer>
  );
}
