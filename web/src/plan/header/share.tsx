import { CheckIcon, CopyIcon, Share2Icon, TriangleAlertIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { apiGet } from "@/plan/api/client";
import { shareLinkSchema, type ShareLink } from "@/plan/api/schema";
import { Popover, PopoverContent, PopoverTrigger } from "@/plan/header/popover";

type Loaded =
  | { kind: "loading" }
  | { kind: "error"; message: string }
  | { kind: "link"; link: ShareLink };

type CopyState = "idle" | "copied" | "failed";

function selectContents(element: HTMLElement | null): void {
  const selection = window.getSelection();
  if (!element || !selection) return;
  const range = document.createRange();
  range.selectNodeContents(element);
  selection.removeAllRanges();
  selection.addRange(range);
}

/**
 * The clipboard API is missing outside secure contexts (a remote plain-HTTP
 * bind), so a failed copy selects the text for a manual copy and says so.
 */
function CopyRow({ text, noun }: { text: string; noun: string }) {
  const [state, setState] = useState<CopyState>("idle");
  const code = useRef<HTMLElement>(null);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setState("copied");
    } catch {
      setState("failed");
      selectContents(code.current);
    }
  };
  const copied = state === "copied";
  const failed = state === "failed";
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center gap-2">
        <code
          ref={code}
          className="min-w-0 flex-1 break-all rounded-md bg-muted px-2 py-1 font-mono text-xs"
        >
          {text}
        </code>
        {/* The name follows the visible text (WCAG 2.5.3) and keeps the two buttons apart. */}
        <Button
          variant="outline"
          size="sm"
          aria-label={copied ? "Copied" : `Copy ${noun}`}
          onClick={() => void copy()}
        >
          {copied ? <CheckIcon aria-hidden="true" /> : <CopyIcon aria-hidden="true" />}
          {copied ? "Copied" : "Copy"}
        </Button>
      </div>
      <p role="status" className={failed ? "text-xs font-medium text-(--tone-blocked)" : "sr-only"}>
        {copied && `Copied the ${noun} to the clipboard.`}
        {failed && `Copy failed. Select the ${noun} and copy it.`}
      </p>
    </div>
  );
}

function ShareBody({ link }: { link: ShareLink }) {
  return (
    <>
      <CopyRow text={link.url} noun="link" />
      {link.bind === "remote" && (
        <p className="flex items-start gap-1.5 text-xs font-medium text-(--tone-warning)">
          <TriangleAlertIcon className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
          This link is plain HTTP. Anyone who can see the traffic can read it.
        </p>
      )}
    </>
  );
}

/**
 * Fetches `GET /api/share` each time the popover opens and drops the link on
 * close: the URL carries the access token, so it is never cached in an atom.
 */
export function Share() {
  const [open, setOpen] = useState(false);
  const [loaded, setLoaded] = useState<Loaded>({ kind: "loading" });

  useEffect(() => {
    if (!open) return;
    const controller = new AbortController();
    void apiGet("/api/share", shareLinkSchema, controller.signal).then((result) => {
      if (controller.signal.aborted) return;
      setLoaded(
        result.ok
          ? { kind: "link", link: result.value }
          : { kind: "error", message: result.message },
      );
    });
    return () => {
      controller.abort();
      setLoaded({ kind: "loading" });
    };
  }, [open]);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button variant="outline" size="sm">
          <Share2Icon aria-hidden="true" />
          Share
        </Button>
      </PopoverTrigger>
      <PopoverContent aria-label="Share link">
        {loaded.kind === "loading" && (
          <p role="status" className="text-xs text-muted-foreground">
            Loading link
          </p>
        )}
        {loaded.kind === "error" && (
          <p role="alert" className="text-xs font-medium text-(--tone-blocked)">
            Could not load the link: {loaded.message}
          </p>
        )}
        {loaded.kind === "link" && <ShareBody link={loaded.link} />}
      </PopoverContent>
    </Popover>
  );
}
