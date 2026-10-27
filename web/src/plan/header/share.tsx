import { CheckIcon, CopyIcon, Share2Icon, TriangleAlertIcon } from "lucide-react";
import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { apiGet } from "@/plan/api/client";
import { shareLinkSchema, type ShareLink } from "@/plan/api/schema";
import { Popover, PopoverContent, PopoverTrigger } from "@/plan/header/popover";

type Loaded =
  | { kind: "loading" }
  | { kind: "error"; message: string }
  | { kind: "link"; link: ShareLink };

function CopyButton({ text, label }: { text: string; label: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  };
  return (
    <Button variant="outline" size="sm" aria-label={label} onClick={() => void copy()}>
      {copied ? <CheckIcon aria-hidden="true" /> : <CopyIcon aria-hidden="true" />}
      {copied ? "Copied" : "Copy"}
    </Button>
  );
}

function CopyRow({ text, label }: { text: string; label: string }) {
  return (
    <div className="flex items-center gap-2">
      <code className="min-w-0 flex-1 break-all rounded-md bg-muted px-2 py-1 font-mono text-xs">
        {text}
      </code>
      <CopyButton text={text} label={label} />
    </div>
  );
}

function ShareBody({ link }: { link: ShareLink }) {
  return (
    <>
      <CopyRow text={link.url} label="Copy link" />
      {link.bind === "loopback" && link.tunnel && (
        <>
          <p className="text-xs text-muted-foreground">
            Works on this machine. From another machine, tunnel first:
          </p>
          <CopyRow text={link.tunnel} label="Copy tunnel command" />
        </>
      )}
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
      <PopoverContent>
        {loaded.kind === "loading" && <p className="text-xs text-muted-foreground">Loading link</p>}
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
