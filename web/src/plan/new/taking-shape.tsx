import { Circle, CircleCheck, Pencil } from "lucide-react";
import { useId, type ReactNode } from "react";

import type { KickoffShape } from "@/plan/api/schema-kickoff";
import { Untrusted } from "@/plan/chat/visible-controls";

const SECTION_STATES = {
  settled: { label: "Settled", Icon: CircleCheck },
  wording: { label: "Being worded", Icon: Pencil },
  open: { label: "Open", Icon: Circle },
} as const;

export function TakingShape({
  shape,
  target,
}: {
  shape: KickoffShape | null;
  target: string | null;
}) {
  const size = shape?.size;
  const sizeLabel = size === "feature" ? "Feature" : "Standalone plan";

  // Design section 8: this panel stays in the UI face; the transcript uses the reading face.
  return (
    <aside
      aria-label="Taking shape"
      className="min-w-0 space-y-5 p-5 font-sans text-sm text-foreground [overflow-wrap:anywhere]"
    >
      <h2 className="text-base font-semibold">Taking shape</h2>
      {shape === null && (
        <p className="text-muted-foreground">
          Nothing yet. The shape appears here as the conversation settles it.
        </p>
      )}
      <ShapeField title="Size">
        <p>{size ? sizeLabel : "Not decided yet"}</p>
      </ShapeField>
      <ShapeName name={shape?.name ?? null} target={target} />
      <ShapeField title="Problem">
        <p className="whitespace-pre-wrap">
          {shape?.problem ? <Untrusted text={shape.problem} /> : "Not stated yet"}
        </p>
      </ShapeField>
      <ShapeField title="Settled decisions">
        <ShapeList items={shape?.decisions ?? []} empty="Nothing settled yet" />
      </ShapeField>
      <ShapeField title="Open questions">
        <ShapeList items={shape?.questions ?? []} empty="No open questions" />
      </ShapeField>
      {shape?.size === "feature" && <ShapeSections sections={shape.sections} />}
    </aside>
  );
}

function ShapeField({ title, children }: { title: string; children: ReactNode }) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="space-y-2">
      <h3 id={headingId} className="text-xs font-medium text-muted-foreground">
        {title}
      </h3>
      {children}
    </section>
  );
}

function ShapeName({ name, target }: { name: string | null; target: string | null }) {
  return (
    <ShapeField title="Name">
      {name ? (
        <p className="font-mono">
          <Untrusted text={name} />
        </p>
      ) : (
        <p className="text-muted-foreground">Not named yet</p>
      )}
      {target !== null && (
        <p className="text-xs text-muted-foreground">
          Will write{" "}
          <span className="font-mono">
            <Untrusted text={target} />
          </span>
        </p>
      )}
    </ShapeField>
  );
}

function ShapeList({ items, empty }: { items: string[]; empty: string }) {
  if (items.length === 0) return <p className="text-muted-foreground">{empty}</p>;
  return (
    <ul className="list-disc space-y-2 pl-4">
      {items.map((text, index) => (
        <li key={`${index}:${text}`} className="whitespace-pre-wrap">
          <Untrusted text={text} />
        </li>
      ))}
    </ul>
  );
}

function ShapeSections({ sections }: { sections: KickoffShape["sections"] }) {
  return (
    <ShapeField title="Sections">
      {sections.length === 0 ? (
        <p className="text-muted-foreground">No sections proposed yet</p>
      ) : (
        <ul className="space-y-3">
          {sections.map(({ title, state }, index) => {
            const { label, Icon } = SECTION_STATES[state];
            return (
              <li key={`${index}:${title}`} className="flex flex-wrap items-start gap-2">
                <span className="min-w-0 flex-1">
                  <Untrusted text={title} />
                </span>
                <span className="inline-flex items-center gap-1 rounded-full border border-border bg-muted px-2 py-0.5 text-xs text-muted-foreground">
                  <Icon aria-hidden="true" className="size-3 shrink-0" />
                  {label}
                </span>
              </li>
            );
          })}
        </ul>
      )}
    </ShapeField>
  );
}
