import { InboxIcon, SearchXIcon } from "lucide-react";

import { EmptyState } from "@/aurora-ui/feedback/EmptyState";
import { HazardPanel } from "@/aurora-ui/feedback/HazardPanel";
import { Button } from "@/components/ui/button";
import { Untrusted } from "@/plan/chat/visible-controls";
import { BusyText } from "@/plan/ui/busy-text";

interface LoadStateProps {
  state: "loading" | "empty" | "not_found" | "error";
  title?: string;
  message?: string;
  action?: { label: string; onClick: () => void };
}

/** `message` is often a server's or an error's text, so it is drawn through Untrusted here. */
export function LoadState({ state, title, message, action }: LoadStateProps) {
  const button = action && <Button onClick={action.onClick}>{action.label}</Button>;
  const text = message ? <Untrusted text={message} /> : undefined;

  if (state === "loading") {
    // The title is the one announcement (the roundel is hidden), set where the content will start.
    const label = title ?? "Loading";
    return (
      <div role="status" aria-label={label}>
        <BusyText className="plan-empty">{label}</BusyText>
        {text && <p>{text}</p>}
        {button}
      </div>
    );
  }

  if (state === "error") {
    return (
      <HazardPanel tone="error" title={title ?? "Something went wrong"}>
        {text && <p>{text}</p>}
        {button}
      </HazardPanel>
    );
  }

  return (
    <EmptyState
      icon={state === "empty" ? InboxIcon : SearchXIcon}
      title={title ?? (state === "empty" ? "Nothing here yet" : "Not found")}
      description={text}
      action={button}
    />
  );
}
