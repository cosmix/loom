import { useAtomValue } from "jotai";
import { useEffect, useState } from "react";

import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import type { ApiResult } from "@/plan/api/client";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { DiffView } from "@/plan/diff/diff-view";
import { toAddress } from "@/plan/files/address";
import { fetchDoc } from "@/plan/files/api";
import { connectionEpochAtom } from "@/plan/state/atoms";
import { LoadState } from "@/plan/ui/load-state";

import { getAtCommit, getBuilt, type BuiltResponse } from "./api";
import { hashesAtom } from "./live";

const LIMITS =
  "Running loom run again on a done plan regenerates its review and moves the build date later. A done plan without a review shows no marker.";
type BuiltNote = BuiltResponse["notes"][number];
type Comparison = { before: string | null; after: string | null };

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
  onClose,
}: {
  label: string;
  section: string;
  commit: string;
  onClose: () => void;
}) {
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent className="flex max-h-[90vh] flex-col overflow-auto sm:max-w-5xl motion-reduce:animate-none">
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
  onOpen,
}: {
  label: string;
  section: string;
  commit: string | null;
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
      <MarkerLink label={label} section={section} commit={commit} onOpen={() => setOpen(true)} />
      {open && commit !== null && (
        <BuildDialog
          label={label}
          section={section}
          commit={commit}
          onClose={() => setOpen(false)}
        />
      )}
    </>
  );
}

export function BuiltMarker({ section }: { section: string }) {
  const epoch = useAtomValue(connectionEpochAtom);
  const hashes = useAtomValue(hashesAtom);
  const [load, setLoad] = useState<ApiResult<BuiltResponse> | null>(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let active = true;
    setLoad(null);
    void getBuilt(section).then((result) => {
      if (active) setLoad(result);
    });
    return () => {
      active = false;
    };
  }, [section, epoch, hashes, attempt]);
  if (!load) return <LoadState state="loading" title="Loading build markers" />;
  if (!load.ok)
    return (
      <LoadState
        state="error"
        title="Build markers did not load"
        message={visibleControls(load.message)}
        action={{ label: "Retry", onClick: () => setAttempt((n) => n + 1) }}
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
