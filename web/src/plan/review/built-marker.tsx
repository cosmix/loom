import { useAtomValue } from "jotai";
import { useEffect, useRef, useState, type RefObject } from "react";

import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import type { ApiResult } from "@/plan/api/client";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { DiffView } from "@/plan/diff/diff-view";
import { toAddress } from "@/plan/files/address";
import { fetchDoc } from "@/plan/files/api";
import { connectionEpochAtom } from "@/plan/state/atoms";
import { BusyText } from "@/plan/ui/busy-text";
import { LoadState } from "@/plan/ui/load-state";

import { getAtCommit, getBuilt, type BuiltResponse } from "./api";
import { returnFocusTo } from "./focus-return";
import { hashesAtom } from "./live";

const LIMITS =
  "Running loom run again on a done plan regenerates its review and moves the build date later. A done plan without a review shows no marker.";
type BuiltNote = BuiltResponse["notes"][number];
type Comparison = { before: string | null; after: string | null };
type Loaded = { section: string; result: ApiResult<BuiltResponse> };

function BuiltDiff({ section, commit }: { section: string; commit: string }) {
  const [load, setLoad] = useState<ApiResult<Comparison> | null>(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let active = true;
    setLoad(null);
    void Promise.all([getAtCommit(commit, section), fetchDoc(section)]).then(([before, after]) => {
      if (!active) return;
      if (!before.ok) setLoad(before);
      else if (!after.ok && after.status !== 404) setLoad(after);
      else
        setLoad({
          ok: true,
          value: { before: before.value.text, after: after.ok ? after.value.text : null },
        });
    });
    return () => {
      active = false;
    };
  }, [section, commit, attempt]);
  if (!load) return <LoadState state="loading" title="Loading the build comparison" />;
  if (!load.ok)
    return (
      <LoadState
        state="error"
        title="The comparison did not load"
        message={visibleControls(load.message)}
        action={{ label: "Retry", onClick: () => setAttempt((n) => n + 1) }}
      />
    );
  return <DiffView path={section} before={load.value.before} after={load.value.after} />;
}

function BuildDialog({
  label,
  section,
  commit,
  link,
  onClose,
}: {
  label: string;
  section: string;
  commit: string;
  /** The marker link that opened the dialog; focus returns to it on close. */
  link: RefObject<HTMLAnchorElement | null>;
  onClose: () => void;
}) {
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent
        className="flex max-h-[90vh] flex-col overflow-auto sm:max-w-5xl motion-reduce:animate-none"
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          returnFocusTo(link.current);
        }}
      >
        <DialogTitle>
          <Untrusted text={label} />
        </DialogTitle>
        <DialogDescription>
          From the last commit before the build date to the current file.{" "}
          <Untrusted text={section} />
        </DialogDescription>
        <BuiltDiff section={section} commit={commit} />
      </DialogContent>
    </Dialog>
  );
}

function MarkerLink({
  label,
  section,
  commit,
  link,
  onOpen,
}: {
  label: string;
  section: string;
  commit: string | null;
  link: RefObject<HTMLAnchorElement | null>;
  onOpen: () => void;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        {commit === null ? (
          <span tabIndex={0} className="text-[13px]" title={LIMITS}>
            <Untrusted text={label} /> · Build comparison unavailable
          </span>
        ) : (
          <a
            ref={link}
            href={`${toAddress(section)}?base_commit=${encodeURIComponent(commit)}`}
            className="text-[13px] underline"
            title={LIMITS}
            onClick={(event) => {
              event.preventDefault();
              onOpen();
            }}
          >
            <Untrusted text={label} />
          </a>
        )}
      </TooltipTrigger>
      <TooltipContent className="motion-reduce:animate-none">{LIMITS}</TooltipContent>
    </Tooltip>
  );
}

function MarkerNote({ note, section }: { note: BuiltNote; section: string }) {
  const [open, setOpen] = useState(false);
  const link = useRef<HTMLAnchorElement>(null);
  if (note.kind === "running") {
    return (
      <p role="note" className="text-[13px]">
        This plan is running; changes here do not reach it{" "}
        <span className="font-mono">
          (<Untrusted text={note.slug} />)
        </span>
      </p>
    );
  }
  const label = `Changed after ${note.slug} was built`;
  const commit = note.base_commit;
  return (
    <>
      <MarkerLink
        label={label}
        section={section}
        commit={commit}
        link={link}
        onOpen={() => setOpen(true)}
      />
      {open && commit !== null && (
        <BuildDialog
          label={label}
          section={section}
          commit={commit}
          link={link}
          onClose={() => setOpen(false)}
        />
      )}
    </>
  );
}

export function BuiltMarker({ section }: { section: string }) {
  const epoch = useAtomValue(connectionEpochAtom);
  const hashes = useAtomValue(hashesAtom);
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [attempt, setAttempt] = useState(0);
  // A refetch (every hashes change, a reconnect) keeps the last result of this section on show:
  // resetting it would flash a placeholder and move the page header under the reader.
  useEffect(() => {
    let active = true;
    void getBuilt(section).then((result) => {
      if (active) setLoaded({ section, result });
    });
    return () => {
      active = false;
    };
  }, [section, epoch, hashes, attempt]);
  const load = loaded?.section === section ? loaded.result : null;
  if (!load)
    return (
      <p role="status" className="text-[13px] text-muted-foreground">
        <BusyText>Loading build markers…</BusyText>
      </p>
    );
  if (!load.ok)
    return (
      <LoadState
        state="error"
        title="Build markers did not load"
        message={visibleControls(load.message)}
        action={{
          label: "Retry",
          onClick: () => {
            setLoaded(null);
            setAttempt((n) => n + 1);
          },
        }}
      />
    );
  if (load.value.notes.length === 0) return null;
  return (
    <TooltipProvider>
      <div className="flex flex-col gap-1">
        {load.value.notes.map((note) => (
          <MarkerNote key={`${section}:${note.kind}:${note.slug}`} note={note} section={section} />
        ))}
      </div>
    </TooltipProvider>
  );
}
